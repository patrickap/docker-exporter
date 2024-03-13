use bollard::{
  container::{
    InspectContainerOptions, ListContainersOptions, MemoryStatsStats, Stats, StatsOptions,
  },
  errors::Error,
  secret::{ContainerState, ContainerSummary},
  Docker,
};
use futures::{
  future,
  stream::{StreamExt, TryStreamExt},
};
use prometheus_client::{
  encoding::EncodeLabelSet,
  metrics::{
    counter::Counter,
    family::Family,
    gauge::{Atomic, Gauge},
  },
  registry::{self, Registry},
};
use std::{
  env,
  sync::{
    atomic::{AtomicI64, AtomicU64},
    Arc,
  },
};

use crate::constant::{
  DOCKER_API_VERSION, DOCKER_CONNECTION_TIMEOUT, DOCKER_HOST_ENV, DOCKER_SOCKET_PATH,
};

pub struct RawMetric {
  pub id: Option<String>,
  pub name: Option<String>,
  pub state: Option<ContainerState>,
  pub stats: Option<Stats>,
}

pub trait DockerExt {
  fn try_connect() -> Result<Docker, Error>;
  async fn collect_metrics(self: Arc<Self>) -> Option<Vec<RawMetric>>;
  async fn list_containers_all(&self) -> Option<Vec<ContainerSummary>>;
  async fn inspect_state(&self, container_name: &str) -> Option<ContainerState>;
  async fn stats_once(&self, container_name: &str) -> Option<Stats>;
  #[cfg(test)]
  fn try_connect_mock() -> Result<Docker, Error>;
}

// TODO: extension trait vs custom impl ?!
impl DockerExt for Docker {
  fn try_connect() -> Result<Docker, Error> {
    match env::var(DOCKER_HOST_ENV) {
      Ok(docker_host) => {
        Docker::connect_with_http(&docker_host, DOCKER_CONNECTION_TIMEOUT, DOCKER_API_VERSION)
      }
      _ => Docker::connect_with_socket(
        DOCKER_SOCKET_PATH,
        DOCKER_CONNECTION_TIMEOUT,
        DOCKER_API_VERSION,
      ),
    }
  }

  async fn collect_metrics(self: Arc<Self>) -> Option<Vec<RawMetric>> {
    let containers = self.list_containers_all().await.unwrap_or_default();

    let result = containers.into_iter().map(|container| {
      let _self = Arc::clone(&self);

      tokio::spawn(async move {
        let container = Arc::new(container);

        let state = {
          let _self = Arc::clone(&_self);
          let container = Arc::clone(&container);
          tokio::spawn(async move { _self.inspect_state(&container.id.as_deref()?).await })
        };

        let stats = {
          let _self = Arc::clone(&_self);
          let container = Arc::clone(&container);
          tokio::spawn(async move { _self.stats_once(&container.id.as_deref()?).await })
        };

        let (state, stats) = tokio::join!(state, stats);
        let (state, stats) = (state.ok().flatten(), stats.ok().flatten());

        RawMetric {
          id: stats.as_ref().map(|s| String::from(&s.id)),
          name: stats.as_ref().map(|s| String::from(&s.name[1..])),
          state,
          stats,
        }
      })
    });

    future::try_join_all(result).await.ok()
  }

  async fn list_containers_all(&self) -> Option<Vec<ContainerSummary>> {
    let options = Some(ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    });

    self.list_containers(options).await.ok()
  }

  async fn inspect_state(&self, container_name: &str) -> Option<ContainerState> {
    let options = Some(InspectContainerOptions {
      ..Default::default()
    });

    self
      .inspect_container(container_name, options)
      .await
      .ok()
      .and_then(|inspect| inspect.state)
  }

  async fn stats_once(&self, container_name: &str) -> Option<Stats> {
    let options = Some(StatsOptions {
      stream: false,
      ..Default::default()
    });

    self
      .stats(container_name, options)
      .take(1)
      .try_next()
      .await
      .ok()
      .flatten()
  }

  #[cfg(test)]
  fn try_connect_mock() -> Result<Docker, Error> {
    // This is currently sufficient for a test as it returns Ok even if the socket is unavailable
    Docker::connect_with_socket("/dev/null", 0, DOCKER_API_VERSION)
  }
}

pub trait StatsExt {
  fn cpu_delta(&self) -> Option<u64>;
  fn cpu_delta_system(&self) -> Option<u64>;
  fn cpu_count(&self) -> Option<u64>;
  fn cpu_utilization(&self) -> Option<f64>;
  fn memory_usage(&self) -> Option<u64>;
  fn memory_total(&self) -> Option<u64>;
  fn memory_utilization(&self) -> Option<f64>;
  fn block_io_total(&self) -> Option<(u64, u64)>;
  fn block_io_tx_total(&self) -> Option<u64>;
  fn block_io_rx_total(&self) -> Option<u64>;
  fn network_total(&self) -> Option<(u64, u64)>;
  fn network_tx_total(&self) -> Option<u64>;
  fn network_rx_total(&self) -> Option<u64>;
}

impl StatsExt for Stats {
  fn cpu_delta(&self) -> Option<u64> {
    Some(self.cpu_stats.cpu_usage.total_usage - self.precpu_stats.cpu_usage.total_usage)
  }

  fn cpu_delta_system(&self) -> Option<u64> {
    Some(self.cpu_stats.system_cpu_usage? - self.precpu_stats.system_cpu_usage?)
  }

  fn cpu_count(&self) -> Option<u64> {
    self.cpu_stats.online_cpus.or(Some(1))
  }

  fn cpu_utilization(&self) -> Option<f64> {
    Some(
      (self.cpu_delta()? as f64 / self.cpu_delta_system()? as f64)
        * self.cpu_count()? as f64
        * 100.0,
    )
  }

  fn memory_usage(&self) -> Option<u64> {
    let memory_cache = match self.memory_stats.stats? {
      MemoryStatsStats::V1(memory_stats) => memory_stats.cache,
      // In cgroup v2, Docker doesn't provide a cache property
      // Unfortunately, there's no simple way to differentiate cache from memory usage
      MemoryStatsStats::V2(_) => 0,
    };

    Some(self.memory_stats.usage? - memory_cache)
  }

  fn memory_total(&self) -> Option<u64> {
    self.memory_stats.limit
  }

  fn memory_utilization(&self) -> Option<f64> {
    Some((self.memory_usage()? as f64 / self.memory_total()? as f64) * 100.0)
  }

  fn block_io_total(&self) -> Option<(u64, u64)> {
    self
      .blkio_stats
      .io_service_bytes_recursive
      .as_ref()?
      .iter()
      .fold(Some((0, 0)), |acc, io| match io.op.as_str() {
        "write" => Some((acc?.0 + io.value, acc?.1)),
        "read" => Some((acc?.0, acc?.1 + io.value)),
        _ => acc,
      })
  }

  fn block_io_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.block_io_total()?;
    Some(tx)
  }

  fn block_io_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.block_io_total()?;
    Some(rx)
  }

  fn network_total(&self) -> Option<(u64, u64)> {
    let network = self.networks.as_ref()?.get("eth0")?;
    Some((network.tx_bytes, network.rx_bytes))
  }

  fn network_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.network_total()?;
    Some(tx)
  }

  fn network_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.network_total()?;
    Some(rx)
  }
}

pub struct Metric<'a, M: registry::Metric + Clone> {
  pub name: &'a str,
  pub help: &'a str,
  pub metric: M,
}

impl<'a, M: registry::Metric + Clone> Metric<'a, M> {
  pub fn new(name: &'a str, help: &'a str, metric: M) -> Self {
    Self { name, help, metric }
  }

  pub fn register(&self, registry: &mut Registry) {
    // Cloning the metric is fine and suggested by the library as it internally uses an Arc
    registry.register(self.name, self.help, self.metric.clone());
  }
}

pub struct Metrics<'a> {
  pub state_running_boolean: Metric<'a, Family<MetricsLabels, Gauge<i64, AtomicI64>>>,
  pub cpu_utilization_percent: Metric<'a, Family<MetricsLabels, Gauge<f64, AtomicU64>>>,
  pub memory_usage_bytes: Metric<'a, Family<MetricsLabels, Gauge<f64, AtomicU64>>>,
  pub memory_bytes_total: Metric<'a, Family<MetricsLabels, Counter<f64, AtomicU64>>>,
  pub memory_utilization_percent: Metric<'a, Family<MetricsLabels, Gauge<f64, AtomicU64>>>,
  pub block_io_tx_bytes_total: Metric<'a, Family<MetricsLabels, Counter<f64, AtomicU64>>>,
  pub block_io_rx_bytes_total: Metric<'a, Family<MetricsLabels, Counter<f64, AtomicU64>>>,
  pub network_tx_bytes_total: Metric<'a, Family<MetricsLabels, Counter<f64, AtomicU64>>>,
  pub network_rx_bytes_total: Metric<'a, Family<MetricsLabels, Counter<f64, AtomicU64>>>,
}

impl<'a> Metrics<'a> {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn aggregate_metric(&self, metric: &RawMetric) {
    let RawMetric {
      id,
      name,
      state,
      stats,
    } = metric;

    let labels = match (id, name) {
      (Some(id), Some(name)) => Some(MetricsLabels {
        container_id: String::from(id),
        container_name: String::from(name),
      }),
      _ => None,
    }
    .unwrap_or_default();

    if let Some(state) = state {
      if let Some(state_running) = state.running {
        self
          .state_running_boolean
          .metric
          .get_or_create(&labels)
          .set(state_running as i64);
      }

      if let Some(true) = state.running {
        if let Some(stats) = stats {
          if let Some(cpu_utilization) = stats.cpu_utilization() {
            self
              .cpu_utilization_percent
              .metric
              .get_or_create(&labels)
              .set(cpu_utilization);
          }

          if let Some(memory_usage) = stats.memory_usage() {
            self
              .memory_usage_bytes
              .metric
              .get_or_create(&labels)
              .set(memory_usage as f64);
          }

          if let Some(memory_total) = stats.memory_total() {
            self
              .memory_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(memory_total as f64);
          }

          if let Some(memory_utilization) = stats.memory_utilization() {
            self
              .memory_utilization_percent
              .metric
              .get_or_create(&labels)
              .set(memory_utilization);
          }

          if let Some(block_io_tx_total) = stats.block_io_tx_total() {
            self
              .block_io_tx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(block_io_tx_total as f64);
          }

          if let Some(block_io_rx_total) = stats.block_io_rx_total() {
            self
              .block_io_rx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(block_io_rx_total as f64);
          }

          if let Some(network_tx_total) = stats.network_tx_total() {
            self
              .network_tx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(network_tx_total as f64);
          }

          if let Some(network_rx_total) = stats.network_rx_total() {
            self
              .network_rx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(network_rx_total as f64);
          }
        }
      }
    }
  }
}

impl<'a> Default for Metrics<'a> {
  fn default() -> Self {
    Self {
      state_running_boolean: Metric::new(
        "state_running_boolean",
        "state running as boolean (1 = true, 0 = false)",
        Default::default(),
      ),
      cpu_utilization_percent: Metric::new(
        "cpu_utilization_percent",
        "cpu utilization in percent",
        Default::default(),
      ),
      memory_usage_bytes: Metric::new(
        "memory_usage_bytes",
        "memory usage in bytes",
        Default::default(),
      ),
      memory_bytes_total: Metric::new("memory_bytes", "memory total in bytes", Default::default()),
      memory_utilization_percent: Metric::new(
        "memory_utilization_percent",
        "memory utilization in percent",
        Default::default(),
      ),
      block_io_tx_bytes_total: Metric::new(
        "block_io_tx_bytes",
        "block io written total in bytes",
        Default::default(),
      ),
      block_io_rx_bytes_total: Metric::new(
        "block_io_rx_bytes",
        "block io read total in bytes",
        Default::default(),
      ),
      network_tx_bytes_total: Metric::new(
        "network_tx_bytes",
        "network sent total in bytes",
        Default::default(),
      ),
      network_rx_bytes_total: Metric::new(
        "network_rx_bytes",
        "network received total in bytes",
        Default::default(),
      ),
    }
  }
}

#[derive(Clone, Debug, Default, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct MetricsLabels {
  pub container_id: String,
  pub container_name: String,
}

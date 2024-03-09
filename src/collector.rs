use bollard::{
  container::{
    InspectContainerOptions, ListContainersOptions, MemoryStatsStats, Stats, StatsOptions,
  },
  models::ContainerState,
  Docker,
};
use futures::{FutureExt, StreamExt, TryStreamExt};
use prometheus_client::{
  collector::Collector,
  encoding::{DescriptorEncoder, EncodeLabelSet, EncodeMetric},
  metrics::{
    counter::Counter,
    family::Family,
    gauge::{Atomic, Gauge},
  },
  registry::{Metric, Registry, Unit},
};
use std::{
  env,
  fmt::Error,
  ops::Deref,
  sync::{
    atomic::{AtomicI64, AtomicU64},
    Arc,
  },
};
use tokio::{
  runtime::Handle,
  sync::mpsc::{self, Sender},
  task,
};

use crate::config::constants::{
  DEFAULT_DOCKER_API_VERSION, DEFAULT_DOCKER_CONNECTION_TIMEOUT, DEFAULT_DOCKER_SOCKET_PATH,
  DOCKER_HOST_ENV, PROMETHEUS_REGISTRY_PREFIX,
};

// TODO: enum for metric names?
// TODO: check again metrics calculation, names etc.
// TODO: http header for open metrics text?
// TODO: tests

pub struct DockerCollector {
  docker: Option<Arc<Docker>>,
  pub registry: Arc<Registry>,
  pub metrics: Arc<ContainerMetrics>,
}

impl DockerCollector {
  pub fn new() -> Self {
    let docker = match env::var(DOCKER_HOST_ENV) {
      Ok(docker_host) => Docker::connect_with_http(
        &docker_host,
        DEFAULT_DOCKER_CONNECTION_TIMEOUT,
        DEFAULT_DOCKER_API_VERSION,
      ),
      _ => Docker::connect_with_socket(
        DEFAULT_DOCKER_SOCKET_PATH,
        DEFAULT_DOCKER_CONNECTION_TIMEOUT,
        DEFAULT_DOCKER_API_VERSION,
      ),
    };

    if let Err(err) = &docker {
      eprintln!("failed to connect to docker daemon: {:?}", err);
    }

    let mut registry = Registry::with_prefix(PROMETHEUS_REGISTRY_PREFIX);
    let metrics = ContainerMetrics::default();

    registry.register(
      "state_running_boolean",
      "state running as boolean (1 = true, 0 = false)",
      Family::clone(&metrics.state_running_boolean),
    );

    registry.register(
      "cpu_utilization_percent",
      "cpu utilization in percent",
      Family::clone(&metrics.cpu_utilization_percent),
    );

    registry.register(
      "memory_usage_bytes",
      "memory usage in bytes",
      Family::clone(&metrics.memory_usage_bytes),
    );

    registry.register(
      "memory_bytes_total",
      "memory total in bytes",
      Family::clone(&metrics.memory_bytes_total),
    );

    registry.register(
      "memory_utilization_percent",
      "memory utilization in percent",
      Family::clone(&metrics.memory_utilization_percent),
    );

    registry.register(
      "block_io_tx_bytes_total",
      "block io written total in bytes",
      Family::clone(&metrics.block_io_tx_bytes_total),
    );

    registry.register(
      "block_io_rx_bytes_total",
      "block io read total in bytes",
      Family::clone(&metrics.block_io_rx_bytes_total),
    );

    registry.register(
      "network_tx_bytes_total",
      "network sent total in bytes",
      Family::clone(&metrics.network_tx_bytes_total),
    );

    registry.register(
      "network_rx_bytes_total",
      "network received total in bytes",
      Family::clone(&metrics.network_rx_bytes_total),
    );

    Self {
      docker: match docker {
        Ok(docker) => Some(Arc::new(docker)),
        _ => None,
      },
      registry: Arc::new(registry),
      metrics: Arc::new(metrics),
    }
  }

  pub async fn process(&self) {
    if let Some(docker) = Option::as_ref(&self.docker) {
      let containers = docker
        .list_containers(Some(ListContainersOptions::<&str> {
          all: true,
          ..Default::default()
        }))
        .await
        .unwrap_or_default();

      for container in containers {
        let docker = Arc::clone(&docker);

        task::spawn(async move {
          let id = Arc::new(container.id);

          let state = {
            let docker = Arc::clone(&docker);
            let id = Arc::clone(&id);

            task::spawn(async move {
              docker
                .inspect_container(
                  id.as_deref().unwrap_or_default(),
                  Some(InspectContainerOptions {
                    ..Default::default()
                  }),
                )
                .await
            })
            .map(|inspect| match inspect {
              Ok(Ok(inspect)) => Arc::new(inspect.state),
              _ => Arc::new(None),
            })
          };

          let stats = {
            let docker = Arc::clone(&docker);
            let id = Arc::clone(&id);

            task::spawn(async move {
              docker
                .stats(
                  id.as_deref().unwrap_or_default(),
                  Some(StatsOptions {
                    stream: false,
                    ..Default::default()
                  }),
                )
                .take(1)
                .try_next()
                .await
            })
            .map(|stats| match stats {
              Ok(Ok(stats)) => Arc::new(stats),
              _ => Arc::new(None),
            })
          };

          let (state, stats) = tokio::join!(state, stats);

          let labels = Arc::new(ContainerMetricLabels {
            container_name: container
              .names
              .and_then(|names| Some(names.join(";")))
              .and_then(|mut name| Some(name.drain(1..).collect()))
              .unwrap_or_default(),
          });

          {
            // let state = Arc::clone(&state);
            // let stats = Arc::clone(&stats);
            // let labels = Arc::clone(&labels);
            task::spawn(async move { &self.collect(state, stats, labels) });
          }
        });
      }
    }
  }

  fn collect(
    &self,
    state: Arc<Option<ContainerState>>,
    stats: Arc<Option<Stats>>,
    labels: Arc<ContainerMetricLabels>,
  ) {
    {
      let metrics = Arc::clone(&self.metrics);
      let state = Arc::clone(&state);
      let labels = Arc::clone(&labels);
      task::spawn(async move {
        let metric = Option::as_ref(&state).and_then(|s| s.running);
        metrics
          .state_running_boolean
          .get_or_create(&labels)
          .set(metric.unwrap_or_default() as i64);
      });
    }

    let running = Option::as_ref(&state)
      .and_then(|s| s.running)
      .unwrap_or_default();

    if running {
      {
        let metrics = Arc::clone(&self.metrics);
        let stats = Arc::clone(&stats);
        let labels = Arc::clone(&labels);
        task::spawn(async move {
          let metric = Option::as_ref(&stats).and_then(|s| s.cpu_utilization());
          if let Some(metric) = metric {
            metrics
              .cpu_utilization_percent
              .get_or_create(&labels)
              .set(metric);
          }
        });
      }

      {
        let metrics = Arc::clone(&self.metrics);
        let stats = Arc::clone(&stats);
        let labels = Arc::clone(&labels);
        task::spawn(async move {
          let metric = Option::as_ref(&stats).and_then(|s| s.memory_usage());
          if let Some(metric) = metric {
            metrics
              .memory_usage_bytes
              .get_or_create(&labels)
              .inner()
              .set(metric as f64);
          }
        });
      }

      {
        let metrics = Arc::clone(&self.metrics);
        let stats = Arc::clone(&stats);
        let labels = Arc::clone(&labels);
        task::spawn(async move {
          let metric = Option::as_ref(&stats).and_then(|s| s.memory_total());
          if let Some(metric) = metric {
            metrics
              .memory_bytes_total
              .get_or_create(&labels)
              .inner()
              .set(metric as f64);
          }
        });
      }

      {
        let metrics = Arc::clone(&self.metrics);
        let stats = Arc::clone(&stats);
        let labels = Arc::clone(&labels);
        task::spawn(async move {
          let metric = Option::as_ref(&stats).and_then(|s| s.memory_utilization());
          if let Some(metric) = metric {
            metrics
              .memory_utilization_percent
              .get_or_create(&labels)
              .inner()
              .set(metric);
          }
        });
      }

      {
        let metrics = Arc::clone(&self.metrics);
        let stats = Arc::clone(&stats);
        let labels = Arc::clone(&labels);
        task::spawn(async move {
          let metric = Option::as_ref(&stats).and_then(|s| s.block_io_tx_total());
          if let Some(metric) = metric {
            metrics
              .block_io_tx_bytes_total
              .get_or_create(&labels)
              .inner()
              .set(metric as f64);
          }
        });
      }

      {
        let metrics = Arc::clone(&self.metrics);
        let stats = Arc::clone(&stats);
        let labels = Arc::clone(&labels);
        task::spawn(async move {
          let metric = Option::as_ref(&stats).and_then(|s| s.block_io_rx_total());
          if let Some(metric) = metric {
            metrics
              .block_io_rx_bytes_total
              .get_or_create(&labels)
              .inner()
              .set(metric as f64);
          }
        });
      }

      {
        let metrics = Arc::clone(&self.metrics);
        let stats = Arc::clone(&stats);
        let labels = Arc::clone(&labels);
        task::spawn(async move {
          let metric = Option::as_ref(&stats).and_then(|s| s.network_tx_total());
          if let Some(metric) = metric {
            metrics
              .network_tx_bytes_total
              .get_or_create(&labels)
              .inner()
              .set(metric as f64);
          }
        });
      }

      {
        let metrics = Arc::clone(&self.metrics);
        let stats = Arc::clone(&stats);
        let labels = Arc::clone(&labels);
        task::spawn(async move {
          let metric = Option::as_ref(&stats).and_then(|s| s.network_rx_total());
          if let Some(metric) = metric {
            metrics
              .network_rx_bytes_total
              .get_or_create(&labels)
              .inner()
              .set(metric as f64);
          }
        });
      }
    }
  }
}

pub struct ContainerMetrics {
  pub state_running_boolean: Family<ContainerMetricLabels, Gauge<i64, AtomicI64>>,
  pub cpu_utilization_percent: Family<ContainerMetricLabels, Gauge<f64, AtomicU64>>,
  pub memory_usage_bytes: Family<ContainerMetricLabels, Gauge<f64, AtomicU64>>,
  pub memory_bytes_total: Family<ContainerMetricLabels, Counter<f64, AtomicU64>>,
  pub memory_utilization_percent: Family<ContainerMetricLabels, Gauge<f64, AtomicU64>>,
  pub block_io_tx_bytes_total: Family<ContainerMetricLabels, Counter<f64, AtomicU64>>,
  pub block_io_rx_bytes_total: Family<ContainerMetricLabels, Counter<f64, AtomicU64>>,
  pub network_tx_bytes_total: Family<ContainerMetricLabels, Counter<f64, AtomicU64>>,
  pub network_rx_bytes_total: Family<ContainerMetricLabels, Counter<f64, AtomicU64>>,
}

impl Default for ContainerMetrics {
  fn default() -> Self {
    Self {
      state_running_boolean: Default::default(),
      cpu_utilization_percent: Default::default(),
      memory_usage_bytes: Default::default(),
      memory_bytes_total: Default::default(),
      memory_utilization_percent: Default::default(),
      block_io_tx_bytes_total: Default::default(),
      block_io_rx_bytes_total: Default::default(),
      network_tx_bytes_total: Default::default(),
      network_rx_bytes_total: Default::default(),
    }
  }
}

#[derive(Clone, Debug, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct ContainerMetricLabels {
  container_name: String,
}

pub trait ContainerStatsExt {
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

impl ContainerStatsExt for Stats {
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

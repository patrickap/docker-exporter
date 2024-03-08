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
  registry::{Metric, Unit},
};
use std::{
  env,
  fmt::Error,
  sync::{atomic::AtomicU64, Arc},
};
use tokio::{
  runtime::Handle,
  sync::mpsc::{self, Sender},
  task,
};

use crate::config::constants::{
  DEFAULT_DOCKER_API_VERSION, DEFAULT_DOCKER_CONNECTION_TIMEOUT, DEFAULT_DOCKER_SOCKET_PATH,
  DOCKER_HOST_ENV,
};

struct ContainerInfo {
  name: Option<String>,
  state: Option<ContainerState>,
  stats: Option<Stats>,
}

impl ContainerInfo {
  pub fn running(&self) -> Option<bool> {
    match self.state {
      Some(state) => state.running,
      _ => None,
    }
  }

  pub fn cpu_delta(&self) -> Option<u64> {
    match self.stats {
      Some(stats) => {
        Some(stats.cpu_stats.cpu_usage.total_usage - stats.precpu_stats.cpu_usage.total_usage)
      }
      _ => None,
    }
  }

  pub fn cpu_delta_system(&self) -> Option<u64> {
    match self.stats {
      Some(stats) => {
        match (
          stats.cpu_stats.system_cpu_usage,
          stats.precpu_stats.system_cpu_usage,
        ) {
          (Some(system_cpu_usage), Some(system_precpu_usage)) => {
            Some(system_cpu_usage - system_precpu_usage)
          }
          _ => None,
        }
      }
      _ => None,
    }
  }

  pub fn cpu_count(&self) -> Option<u64> {
    match self.stats {
      Some(stats) => stats.cpu_stats.online_cpus.or(Some(1)),
      _ => None,
    }
  }

  pub fn cpu_utilization(&self) -> Option<f64> {
    match (self.cpu_delta(), self.cpu_delta_system(), self.cpu_count()) {
      (Some(cpu_delta), Some(cpu_delta_system), Some(cpu_count)) => {
        Some((cpu_delta as f64 / cpu_delta_system as f64) * cpu_count as f64 * 100.0)
      }
      _ => None,
    }
  }

  pub fn memory_usage(&self) -> Option<u64> {
    match self.stats {
      Some(stats) => {
        match (stats.memory_stats.usage, stats.memory_stats.stats) {
          (Some(memory_usage), Some(MemoryStatsStats::V1(memory_stats))) => {
            Some(memory_usage - memory_stats.cache)
          }
          (Some(memory_usage), Some(MemoryStatsStats::V2(_))) => {
            // In cgroup v2, Docker doesn't provide a cache property
            // Unfortunately, there's no simple way to differentiate cache from memory usage
            Some(memory_usage - 0)
          }
          _ => None,
        }
      }
      _ => None,
    }
  }

  pub fn memory_total(&self) -> Option<u64> {
    match self.stats {
      Some(stats) => stats.memory_stats.limit,
      _ => None,
    }
  }

  pub fn memory_utilization(&self) -> Option<f64> {
    match (self.memory_usage(), self.memory_total()) {
      (Some(memory_usage), Some(memory_total)) => {
        Some((memory_usage as f64 / memory_total as f64) * 100.0)
      }
      _ => None,
    }
  }

  pub fn block_io_total(&self) -> (Option<u64>, Option<u64>) {
    match self.stats {
      Some(stats) => match stats.blkio_stats.io_service_bytes_recursive {
        Some(io) => {
          let (tx, rx) = io.iter().fold((0, 0), |acc, io| match io.op.as_str() {
            "write" => (acc.0 + io.value, acc.1),
            "read" => (acc.0, acc.1 + io.value),
            _ => acc,
          });

          (Some(tx), Some(rx))
        }
        _ => (None, None),
      },
      _ => (None, None),
    }
  }

  pub fn block_io_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.block_io_total();
    tx
  }

  pub fn block_io_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.block_io_total();
    rx
  }

  pub fn network_total(&self) -> (Option<u64>, Option<u64>) {
    match self.stats {
      Some(stats) => match stats.networks {
        Some(networks) => match networks.get("eth0") {
          Some(eth0) => (Some(eth0.tx_bytes), Some(eth0.rx_bytes)),
          _ => (None, None),
        },
        _ => (None, None),
      },
      _ => (None, None),
    }
  }

  pub fn network_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.network_total();
    tx
  }

  pub fn network_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.network_total();
    rx
  }
}

pub struct ContainerMetric<M: Metric> {
  name: String,
  help: String,
  metric: Family<ContainerMetricLabels, M>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ContainerMetricLabels {
  container_name: String,
}

pub trait ContainerMetricBuilder {
  type MetricType: Metric;

  fn new(info: Arc<ContainerInfo>) -> Option<ContainerMetric<Self::MetricType>>;
}

pub struct ContainerRunningMetric {}

impl ContainerMetricBuilder for ContainerRunningMetric {
  type MetricType = Gauge;

  fn new(info: Arc<ContainerInfo>) -> Option<ContainerMetric<Self::MetricType>> {
    match (info.name, info.running()) {
      (Some(name), Some(running)) => {
        let metric = Family::<_, Self::MetricType>::default();

        metric
          .get_or_create(&ContainerMetricLabels {
            container_name: name,
          })
          .set(running as i64);

        Some(ContainerMetric {
          name: String::from("container_running_boolean"),
          help: String::from("container running as boolean (1 = true, 0 = false)"),
          metric,
        })
      }
      _ => None,
    }
  }
}

pub struct ContainerCpuUtilizationMetric {}

impl ContainerMetricBuilder for ContainerCpuUtilizationMetric {
  type MetricType = Gauge<f64, AtomicU64>;

  fn new(info: Arc<ContainerInfo>) -> Option<ContainerMetric<Self::MetricType>> {
    match (info.name, info.cpu_utilization()) {
      (Some(name), Some(cpu_utilization)) => {
        let metric = Family::<_, Self::MetricType>::default();

        metric
          .get_or_create(&ContainerMetricLabels {
            container_name: name,
          })
          .set(cpu_utilization);

        Some(ContainerMetric {
          name: String::from("cpu_utilization_percent"),
          help: String::from("cpu utilization in percent"),
          metric,
        })
      }
      _ => None,
    }
  }
}

pub struct ContainerMemoryUsageMetric {}

impl ContainerMetricBuilder for ContainerMemoryUsageMetric {
  type MetricType = Gauge<f64, AtomicU64>;

  fn new(info: Arc<ContainerInfo>) -> Option<ContainerMetric<Self::MetricType>> {
    match (info.name, info.memory_usage()) {
      (Some(name), Some(memory_usage)) => {
        let metric = Family::<_, Self::MetricType>::default();

        metric
          .get_or_create(&ContainerMetricLabels {
            container_name: name,
          })
          .set(memory_usage as f64);

        Some(ContainerMetric {
          name: String::from("memory_usage_bytes"),
          help: String::from("memory usage in bytes"),
          metric,
        })
      }
      _ => None,
    }
  }
}

pub struct ContainerMemoryTotalMetric {}

impl ContainerMetricBuilder for ContainerMemoryTotalMetric {
  type MetricType = Counter<f64, AtomicU64>;

  fn new(info: Arc<ContainerInfo>) -> Option<ContainerMetric<Self::MetricType>> {
    match (info.name, info.memory_total()) {
      (Some(name), Some(memory_total)) => {
        let metric = Family::<_, Self::MetricType>::default();

        metric
          .get_or_create(&ContainerMetricLabels {
            container_name: name,
          })
          .inner()
          .set(memory_total as f64);

        Some(ContainerMetric {
          name: String::from("memory_bytes"),
          help: String::from("memory total in bytes"),
          metric,
        })
      }
      _ => None,
    }
  }
}

// TODO: make ContainerMetricBuilder generic so one can instanciate like ContainerMetricBuilder::<Type>::new()

pub struct ContainerMemoryUtilizationMetric {}

impl ContainerMetricBuilder for ContainerMemoryUtilizationMetric {
  type MetricType = Gauge<f64, AtomicU64>;

  fn new(info: Arc<ContainerInfo>) -> Option<ContainerMetric<Self::MetricType>> {
    match (info.name, info.memory_utilization()) {
      (Some(name), Some(memory_utilization)) => {
        let metric = Family::<_, Self::MetricType>::default();

        metric
          .get_or_create(&ContainerMetricLabels {
            container_name: name,
          })
          .set(memory_utilization);

        Some(ContainerMetric {
          name: String::from("memory_utilization_percent"),
          help: String::from("memory utilization in percent"),
          metric,
        })
      }
      _ => None,
    }
  }
}

pub struct ContainerBlockIoTxMetric {}

impl ContainerMetricBuilder for ContainerBlockIoTxMetric {
  type MetricType = Counter<f64, AtomicU64>;

  fn new(info: Arc<ContainerInfo>) -> Option<ContainerMetric<Self::MetricType>> {
    match (info.name, info.block_io_tx_total()) {
      (Some(name), Some(block_io_tx_total)) => {
        let metric = Family::<_, Self::MetricType>::default();

        metric
          .get_or_create(&ContainerMetricLabels {
            container_name: name,
          })
          .inner()
          .set(block_io_tx_total as f64);

        Some(ContainerMetric {
          name: String::from("block_io_tx_bytes"),
          help: String::from("block io written total in bytes"),
          metric,
        })
      }
      _ => None,
    }
  }
}

pub struct ContainerBlockIoRxMetric {}

impl ContainerMetricBuilder for ContainerBlockIoRxMetric {
  type MetricType = Counter<f64, AtomicU64>;

  fn new(info: Arc<ContainerInfo>) -> Option<ContainerMetric<Self::MetricType>> {
    match (info.name, info.block_io_rx_total()) {
      (Some(name), Some(block_io_rx_total)) => {
        let metric = Family::<_, Self::MetricType>::default();

        metric
          .get_or_create(&ContainerMetricLabels {
            container_name: name,
          })
          .inner()
          .set(block_io_rx_total as f64);

        Some(ContainerMetric {
          name: String::from("block_io_rx_bytes"),
          help: String::from("block io read total in bytes"),
          metric,
        })
      }
      _ => None,
    }
  }
}

pub struct ContainerNetworkTxMetric {}

impl ContainerMetricBuilder for ContainerNetworkTxMetric {
  type MetricType = Counter<f64, AtomicU64>;

  fn new(info: Arc<ContainerInfo>) -> Option<ContainerMetric<Self::MetricType>> {
    match (info.name, info.network_tx_total()) {
      (Some(name), Some(network_tx_total)) => {
        let metric = Family::<_, Self::MetricType>::default();

        metric
          .get_or_create(&ContainerMetricLabels {
            container_name: name,
          })
          .inner()
          .set(network_tx_total as f64);

        Some(ContainerMetric {
          name: String::from("network_tx_bytes"),
          help: String::from("network sent total in bytes"),
          metric,
        })
      }
      _ => None,
    }
  }
}

pub struct ContainerNetworkRxMetric {}

impl ContainerMetricBuilder for ContainerNetworkRxMetric {
  type MetricType = Counter<f64, AtomicU64>;

  fn new(info: Arc<ContainerInfo>) -> Option<ContainerMetric<Self::MetricType>> {
    match (info.name, info.network_rx_total()) {
      (Some(name), Some(network_rx_total)) => {
        let metric = Family::<_, Self::MetricType>::default();

        metric
          .get_or_create(&ContainerMetricLabels {
            container_name: name,
          })
          .inner()
          .set(network_rx_total as f64);

        Some(ContainerMetric {
          name: String::from("network_rx_bytes"),
          help: String::from("network received total in bytes"),
          metric,
        })
      }
      _ => None,
    }
  }
}

pub trait ContainerMetricCollector<S> {
  type Metric;

  fn new() -> Self;
  async fn collect(&self, source: Arc<S>, tx: Arc<Sender<Self::Metric>>);
}

pub struct DockerCollector {}

impl ContainerMetricCollector<Docker> for DockerCollector {
  type Metric = ContainerMetric<Box<dyn Metric>>;

  fn new() -> Self {
    Self {}
  }

  async fn collect(&self, docker: Arc<Docker>, tx: Arc<Sender<Self::Metric>>) {
    let containers = docker
      .list_containers(Some(ListContainersOptions::<&str> {
        all: true,
        ..Default::default()
      }))
      .await
      .unwrap_or_default();

    for container in containers {
      let docker = Arc::clone(&docker);
      let tx = Arc::clone(&tx);
      let id = Arc::new(container.id);

      task::spawn(async move {
        let name = container
          .names
          .and_then(|names| Some(names.join(";")))
          .and_then(|mut name| Some(name.drain(1..).collect()));

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
            Ok(Ok(inspect)) => inspect.state,
            _ => None,
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
            Ok(Ok(stats)) => stats,
            _ => None,
          })
        };

        let (state, stats) = tokio::join!(state, stats);

        let info = Arc::new(ContainerInfo { name, state, stats });

        task::spawn(async {
          if let Some(metric) = ContainerRunningMetric::new(Arc::clone(&info)) {
            tx.send(metric).await?
          }
          Ok(())
        });

        // if let Some(state) = &*state {
        //   if let Some(true) = state.running {
        //     task::spawn(Self::new_cpu_metric(
        //       Arc::clone(&name),
        //       Arc::clone(&stats),
        //       Arc::clone(&tx),
        //     ));

        //     task::spawn(Self::new_memory_metric(
        //       Arc::clone(&name),
        //       Arc::clone(&stats),
        //       Arc::clone(&tx),
        //     ));

        //     task::spawn(Self::new_block_io_metric(
        //       Arc::clone(&name),
        //       Arc::clone(&stats),
        //       Arc::clone(&tx),
        //     ));

        //     task::spawn(Self::new_network_metric(
        //       Arc::clone(&name),
        //       Arc::clone(&stats),
        //       Arc::clone(&tx),
        //     ));
        //   }
        // }
      });
    }
  }
}

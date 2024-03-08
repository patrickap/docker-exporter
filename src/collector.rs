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
    self.state?.running
  }

  pub fn cpu_delta(&self) -> Option<u64> {
    Some(
      self.stats?.cpu_stats.cpu_usage.total_usage - self.stats?.precpu_stats.cpu_usage.total_usage,
    )
  }

  pub fn cpu_delta_system(&self) -> Option<u64> {
    Some(self.stats?.cpu_stats.system_cpu_usage? - self.stats?.precpu_stats.system_cpu_usage?)
  }

  pub fn cpu_count(&self) -> Option<u64> {
    self.stats?.cpu_stats.online_cpus.or(Some(1))
  }

  pub fn cpu_utilization(&self) -> Option<f64> {
    Some(
      (self.cpu_delta()? as f64 / self.cpu_delta_system()? as f64)
        * self.cpu_count()? as f64
        * 100.0,
    )
  }

  pub fn memory_usage(&self) -> Option<u64> {
    let memory_cache = match self.stats?.memory_stats.stats? {
      MemoryStatsStats::V1(memory_stats) => memory_stats.cache,
      // In cgroup v2, Docker doesn't provide a cache property
      // Unfortunately, there's no simple way to differentiate cache from memory usage
      MemoryStatsStats::V2(_) => 0,
      _ => 0,
    };

    Some(self.stats?.memory_stats.usage? - memory_cache)
  }

  pub fn memory_total(&self) -> Option<u64> {
    self.stats?.memory_stats.limit
  }

  pub fn memory_utilization(&self) -> Option<f64> {
    Some((self.memory_usage()? as f64 / self.memory_total()? as f64) * 100.0)
  }

  pub fn block_io_total(&self) -> Option<(u64, u64)> {
    self
      .stats?
      .blkio_stats
      .io_service_bytes_recursive?
      .iter()
      .fold(Some((0, 0)), |acc, io| match io.op.as_str() {
        "write" => Some((acc?.0 + io.value, acc?.1)),
        "read" => Some((acc?.0, acc?.1 + io.value)),
        _ => acc,
      })
  }

  pub fn block_io_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.block_io_total()?;
    Some(tx)
  }

  pub fn block_io_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.block_io_total()?;
    Some(rx)
  }

  pub fn network_total(&self) -> Option<(u64, u64)> {
    let network = self.stats?.networks?.get("eth0")?;
    Some((network.tx_bytes, network.rx_bytes))
  }

  pub fn network_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.network_total()?;
    Some(tx)
  }

  pub fn network_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.network_total()?;
    Some(rx)
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

use bollard::{container::Stats, models::ContainerState, Docker};
use futures::future;
use prometheus_client::{
  encoding::EncodeLabelSet,
  metrics::{
    counter::Counter,
    family::Family,
    gauge::{Atomic, Gauge},
  },
  registry::{self, Registry},
};
use std::sync::{
  atomic::{AtomicI64, AtomicU64},
  Arc,
};
use tokio::task::JoinError;

use crate::extension::{DockerContainerExt, DockerStatsExt};

pub trait Collector {
  type Provider: MetricProvider;

  fn new(provider: Arc<Self::Provider>) -> Self;
  async fn collect(&self) -> <Self::Provider as MetricProvider>::Data;
}

pub struct DockerCollector {
  provider: Arc<Docker>,
}

impl Collector for DockerCollector {
  type Provider = Docker;

  fn new(provider: Arc<Self::Provider>) -> Self {
    Self { provider }
  }

  async fn collect(&self) -> <Self::Provider as MetricProvider>::Data {
    Arc::clone(&self.provider).get_data().await
  }
}

pub trait Metric<M: registry::Metric + Clone> {
  fn new(name: &str, help: &str, metric: M) -> Self;
  fn register(&self, registry: &mut Registry);
}

pub struct DockerMetric<M: registry::Metric + Clone> {
  pub name: String,
  pub help: String,
  pub metric: M,
}

impl<M: registry::Metric + Clone> Metric<M> for DockerMetric<M> {
  fn new(name: &str, help: &str, metric: M) -> Self {
    Self {
      name: String::from(name),
      help: String::from(help),
      metric,
    }
  }

  fn register(&self, registry: &mut Registry) {
    // Cloning the metric is fine and suggested by the library
    registry.register(&self.name, &self.help, self.metric.clone());
  }
}

pub trait MetricProvider {
  type Data;

  async fn get_data(self: Arc<Self>) -> Self::Data;
}

impl MetricProvider for Docker {
  type Data = Result<Vec<(Option<ContainerState>, Option<Stats>)>, JoinError>;

  async fn get_data(self: Arc<Self>) -> Self::Data {
    let docker = Arc::clone(&self);
    let containers = docker.list_containers_all().await.unwrap_or_default();

    let tasks = containers.into_iter().map(|c| {
      let docker = Arc::clone(&docker);

      tokio::spawn(async move {
        let id = c.id.as_deref().unwrap_or_default();
        tokio::join!(docker.inspect_container_state(&id), docker.stats_once(&id))
      })
    });

    future::try_join_all(tasks).await
  }
}

pub trait Metrics {
  type Data: From<<Docker as MetricProvider>::Data>;

  fn new() -> Self;
  fn process(&self, data: Self::Data);
}

pub struct DockerMetrics {
  pub state_running_boolean: DockerMetric<Family<DockerMetricLabels, Gauge<i64, AtomicI64>>>,
  pub cpu_utilization_percent: DockerMetric<Family<DockerMetricLabels, Gauge<f64, AtomicU64>>>,
  pub memory_usage_bytes: DockerMetric<Family<DockerMetricLabels, Gauge<f64, AtomicU64>>>,
  pub memory_bytes_total: DockerMetric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  pub memory_utilization_percent: DockerMetric<Family<DockerMetricLabels, Gauge<f64, AtomicU64>>>,
  pub block_io_tx_bytes_total: DockerMetric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  pub block_io_rx_bytes_total: DockerMetric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  pub network_tx_bytes_total: DockerMetric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  pub network_rx_bytes_total: DockerMetric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
}

impl Metrics for DockerMetrics {
  type Data = <Docker as MetricProvider>::Data;

  fn new() -> Self {
    Default::default()
  }

  fn process(&self, data: Self::Data) {
    for (state, stats) in data.unwrap_or_default() {
      let id = stats.as_ref().and_then(|s| s.id());
      let name = stats.as_ref().and_then(|s| s.name());
      let labels = match (id, name) {
        (Some(id), Some(name)) => Some(DockerMetricLabels {
          container_id: id,
          container_name: name,
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
}

impl Default for DockerMetrics {
  fn default() -> Self {
    Self {
      state_running_boolean: DockerMetric::new(
        "state_running_boolean",
        "state running as boolean (1 = true, 0 = false)",
        Default::default(),
      ),
      cpu_utilization_percent: DockerMetric::new(
        "cpu_utilization_percent",
        "cpu utilization in percent",
        Default::default(),
      ),
      memory_usage_bytes: DockerMetric::new(
        "memory_usage_bytes",
        "memory usage in bytes",
        Default::default(),
      ),
      memory_bytes_total: DockerMetric::new(
        "memory_bytes",
        "memory total in bytes",
        Default::default(),
      ),
      memory_utilization_percent: DockerMetric::new(
        "memory_utilization_percent",
        "memory utilization in percent",
        Default::default(),
      ),
      block_io_tx_bytes_total: DockerMetric::new(
        "block_io_tx_bytes",
        "block io written total in bytes",
        Default::default(),
      ),
      block_io_rx_bytes_total: DockerMetric::new(
        "block_io_rx_bytes",
        "block io read total in bytes",
        Default::default(),
      ),
      network_tx_bytes_total: DockerMetric::new(
        "network_tx_bytes",
        "network sent total in bytes",
        Default::default(),
      ),
      network_rx_bytes_total: DockerMetric::new(
        "network_rx_bytes",
        "network received total in bytes",
        Default::default(),
      ),
    }
  }
}

#[derive(Clone, Debug, Default, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct DockerMetricLabels {
  pub container_id: String,
  pub container_name: String,
}

// #[cfg(test)]
// mod tests {
//   use bollard::Docker;

//   use super::*;
//   use crate::extension::DockerExt;

//   #[tokio::test]
//   async fn it_collects_metrics() {
//     let docker = Docker::try_connect_mock().unwrap();
//     let collector = DockerCollector::new(Arc::new(docker));
//     let output = collector.collect().await.unwrap();
//     let expected = Vec::from([]);
//     assert_eq!(output, expected)
//   }

//   #[test]
//   fn it_processes_metrics() {}
// }

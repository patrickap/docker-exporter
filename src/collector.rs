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

use crate::extension::{DockerExt, DockerStatsExt, RegistryExt};

pub trait Collector {
  type Output;

  async fn collect(&self) -> Self::Output;
}

pub struct DockerCollector<D: DockerExt> {
  docker: Arc<D>,
}

impl<D: DockerExt> DockerCollector<D> {
  pub fn new(docker: Arc<D>) -> Self {
    Self { docker }
  }
}

impl<D: DockerExt + Send + Sync + 'static> Collector for DockerCollector<D> {
  type Output = Vec<(Option<ContainerState>, Option<Stats>)>;

  async fn collect(&self) -> Self::Output {
    let docker = Arc::clone(&self.docker);
    let containers = docker.list_containers_all().await.unwrap_or_default();

    let tasks = containers.into_iter().map(|c| {
      let docker = Arc::clone(&docker);

      tokio::spawn(async move {
        let id = c.id.as_deref().unwrap_or_default();
        tokio::join!(docker.inspect_container_state(&id), docker.stats_once(&id))
      })
    });

    future::try_join_all(tasks).await.unwrap_or_default()
  }
}

pub struct Metric<M: registry::Metric> {
  pub name: String,
  pub help: String,
  pub metric: M,
}

impl<M: registry::Metric + Clone> Metric<M> {
  pub fn new(name: &str, help: &str, metric: M) -> Self {
    Self {
      name: String::from(name),
      help: String::from(help),
      metric,
    }
  }
}

pub trait Metrics {
  type Origin: Collector;
  type Input: From<<Self::Origin as Collector>::Output>;

  fn register(&self, registry: &mut Registry);
  fn update(&self, input: Self::Input);
}

pub struct DockerMetrics {
  state_running_boolean: Metric<Family<DockerMetricLabels, Gauge<i64, AtomicI64>>>,
  cpu_utilization_percent: Metric<Family<DockerMetricLabels, Gauge<f64, AtomicU64>>>,
  memory_usage_bytes: Metric<Family<DockerMetricLabels, Gauge<f64, AtomicU64>>>,
  memory_bytes_total: Metric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  memory_utilization_percent: Metric<Family<DockerMetricLabels, Gauge<f64, AtomicU64>>>,
  block_io_tx_bytes_total: Metric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  block_io_rx_bytes_total: Metric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  network_tx_bytes_total: Metric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  network_rx_bytes_total: Metric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
}

impl DockerMetrics {
  pub fn new() -> Self {
    Default::default()
  }
}

impl Metrics for DockerMetrics {
  type Origin = DockerCollector<Docker>;
  type Input = <Self::Origin as Collector>::Output;

  fn register(&self, registry: &mut Registry) {
    registry.register_metric(&self.state_running_boolean);
    registry.register_metric(&self.cpu_utilization_percent);
    registry.register_metric(&self.memory_usage_bytes);
    registry.register_metric(&self.memory_bytes_total);
    registry.register_metric(&self.memory_utilization_percent);
    registry.register_metric(&self.block_io_tx_bytes_total);
    registry.register_metric(&self.block_io_rx_bytes_total);
    registry.register_metric(&self.network_tx_bytes_total);
    registry.register_metric(&self.network_rx_bytes_total);
  }

  fn update(&self, input: Self::Input) {
    for (state, stats) in input {
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

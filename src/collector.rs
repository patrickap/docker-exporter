use bollard::{
  container::{
    InspectContainerOptions, ListContainersOptions, Stats as ContainerStats, StatsOptions,
  },
  models::{ContainerState, ContainerSummary},
  Docker,
};
use futures::{future, Future};
use futures::{StreamExt, TryStreamExt};
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

use crate::extension::DockerStatsExt;

pub trait Collector {
  fn new(docker: Arc<Docker>) -> Self;
  async fn collect(
    &self,
    container: Arc<impl Container + Send + Sync + 'static>,
  ) -> Result<Vec<(Option<ContainerState>, Option<ContainerStats>)>, JoinError>;
}

pub struct DockerCollector {
  docker: Arc<Docker>,
}

impl Collector for DockerCollector {
  fn new(docker: Arc<Docker>) -> Self {
    Self { docker }
  }

  async fn collect(
    &self,
    container: Arc<impl Container + Send + Sync + 'static>,
  ) -> Result<Vec<(Option<ContainerState>, Option<ContainerStats>)>, JoinError> {
    let docker = Arc::clone(&self.docker);
    let containers = container.get_all(&docker).await.unwrap_or_default();

    let tasks = containers.into_iter().map(|c| {
      let docker = Arc::clone(&docker);
      let container = Arc::clone(&container);

      tokio::spawn(async move {
        let (state, stats) = tokio::join!(
          container.get_state(&docker, &c.id),
          container.get_stats(&docker, &c.id),
        );

        (state, stats)
      })
    });

    future::try_join_all(tasks).await
  }
}

pub trait Container {
  fn new() -> Self;
  async fn get_all(&self, docker: &Docker) -> Option<Vec<ContainerSummary>>;
  fn get_state(
    &self,
    docker: &Docker,
    name: &Option<String>,
  ) -> impl Future<Output = Option<ContainerState>> + Send;
  fn get_stats(
    &self,
    docker: &Docker,
    name: &Option<String>,
  ) -> impl Future<Output = Option<ContainerStats>> + Send;
}

pub struct DockerContainer {}

impl Container for DockerContainer {
  fn new() -> Self {
    Self {}
  }

  async fn get_all(&self, docker: &Docker) -> Option<Vec<ContainerSummary>> {
    let options = Some(ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    });

    docker.list_containers(options).await.ok()
  }

  async fn get_state(&self, docker: &Docker, name: &Option<String>) -> Option<ContainerState> {
    let name = name.as_deref().unwrap_or_default();
    let options = Some(InspectContainerOptions {
      ..Default::default()
    });

    docker
      .inspect_container(name, options)
      .await
      .ok()
      .and_then(|i| i.state)
  }

  async fn get_stats(&self, docker: &Docker, name: &Option<String>) -> Option<ContainerStats> {
    let name = name.as_deref().unwrap_or_default();
    let options = Some(StatsOptions {
      stream: false,
      ..Default::default()
    });

    docker
      .stats(name, options)
      .take(1)
      .try_next()
      .await
      .ok()
      .flatten()
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
    // Cloning the metric is fine and suggested by the library as it internally uses an Arc
    registry.register(&self.name, &self.help, self.metric.clone());
  }
}

pub trait Metrics {
  fn new() -> Self;
  fn process(&self, state: &Option<ContainerState>, stats: &Option<ContainerStats>);
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
  fn new() -> Self {
    Default::default()
  }

  fn process(&self, state: &Option<ContainerState>, stats: &Option<ContainerStats>) {
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

#[cfg(test)]
mod tests {
  #[tokio::test]
  async fn it_collects_metrics() {}

  #[test]
  fn it_processes_metrics() {}
}

use bollard::{
  container::{InspectContainerOptions, ListContainersOptions, Stats, StatsOptions},
  models::{ContainerState, ContainerSummary},
  Docker,
};
use futures::future;
use futures::{StreamExt, TryStreamExt};
use prometheus_client::{
  encoding::EncodeLabelSet,
  metrics::{
    counter::Counter,
    family::Family,
    gauge::{Atomic, Gauge},
  },
  registry::{Metric, Registry},
};
use std::sync::{
  atomic::{AtomicI64, AtomicU64},
  Arc,
};
use tokio::task::JoinError;

use crate::extension::DockerStatsExt;

pub struct DockerCollector {
  pub docker: Docker,
  pub metrics: DockerMetrics,
}

impl DockerCollector {
  pub fn new(docker: Docker, metrics: DockerMetrics) -> Self {
    Self { docker, metrics }
  }

  pub async fn collect_metrics(
    self: Arc<Self>,
  ) -> Result<Vec<(Option<ContainerState>, Option<Stats>)>, JoinError> {
    let containers = self.get_containers().await.unwrap_or_default();

    let result = containers.into_iter().map(|container| {
      let _self = Arc::clone(&self);

      tokio::spawn(async move {
        let _self = Arc::clone(&_self);
        let container = Arc::new(container);

        let state = {
          let _self = Arc::clone(&_self);
          let container = Arc::clone(&container);
          tokio::spawn(async move { _self.get_container_state(&container.id).await })
        };

        let stats = {
          let _self = Arc::clone(&_self);
          let container = Arc::clone(&container);
          tokio::spawn(async move { _self.get_container_stats(&container.id).await })
        };

        let (state, stats) = tokio::join!(state, stats);

        (state.ok().flatten(), stats.ok().flatten())
      })
    });

    future::try_join_all(result).await
  }

  pub fn process_metrics(&self, state: &Option<ContainerState>, stats: &Option<Stats>) {
    let id = stats.as_ref().and_then(|s| s.id());
    let name = stats.as_ref().and_then(|s| s.name());
    let labels = match (id, name) {
      (Some(id), Some(name)) => Some(DockerLabels {
        container_id: id,
        container_name: name,
      }),
      _ => None,
    }
    .unwrap_or_default();

    if let Some(state) = state {
      if let Some(state_running) = state.running {
        self
          .metrics
          .state_running_boolean
          .metric
          .get_or_create(&labels)
          .set(state_running as i64);
      }

      if let Some(true) = state.running {
        if let Some(stats) = stats {
          if let Some(cpu_utilization) = stats.cpu_utilization() {
            self
              .metrics
              .cpu_utilization_percent
              .metric
              .get_or_create(&labels)
              .set(cpu_utilization);
          }

          if let Some(memory_usage) = stats.memory_usage() {
            self
              .metrics
              .memory_usage_bytes
              .metric
              .get_or_create(&labels)
              .set(memory_usage as f64);
          }

          if let Some(memory_total) = stats.memory_total() {
            self
              .metrics
              .memory_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(memory_total as f64);
          }

          if let Some(memory_utilization) = stats.memory_utilization() {
            self
              .metrics
              .memory_utilization_percent
              .metric
              .get_or_create(&labels)
              .set(memory_utilization);
          }

          if let Some(block_io_tx_total) = stats.block_io_tx_total() {
            self
              .metrics
              .block_io_tx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(block_io_tx_total as f64);
          }

          if let Some(block_io_rx_total) = stats.block_io_rx_total() {
            self
              .metrics
              .block_io_rx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(block_io_rx_total as f64);
          }

          if let Some(network_tx_total) = stats.network_tx_total() {
            self
              .metrics
              .network_tx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(network_tx_total as f64);
          }

          if let Some(network_rx_total) = stats.network_rx_total() {
            self
              .metrics
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

  async fn get_containers(&self) -> Option<Vec<ContainerSummary>> {
    let options = Some(ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    });

    self.docker.list_containers(options).await.ok()
  }

  async fn get_container_state(&self, container_name: &Option<String>) -> Option<ContainerState> {
    let name = container_name.as_deref().unwrap_or_default();
    let options = Some(InspectContainerOptions {
      ..Default::default()
    });

    self
      .docker
      .inspect_container(name, options)
      .await
      .ok()
      .and_then(|inspect| inspect.state)
  }

  async fn get_container_stats(&self, container_name: &Option<String>) -> Option<Stats> {
    let name = container_name.as_deref().unwrap_or_default();
    let options = Some(StatsOptions {
      stream: false,
      ..Default::default()
    });

    self
      .docker
      .stats(name, options)
      .take(1)
      .try_next()
      .await
      .ok()
      .flatten()
  }
}

pub struct DockerMetric<M: Metric + Clone> {
  pub name: String,
  pub help: String,
  pub metric: M,
}

impl<M: Metric + Clone> DockerMetric<M> {
  pub fn new(name: &str, help: &str, metric: M) -> Self {
    Self {
      name: String::from(name),
      help: String::from(help),
      metric,
    }
  }

  pub fn register(&self, registry: &mut Registry) {
    // Cloning the metric is fine and suggested by the library as it internally uses an Arc
    registry.register(&self.name, &self.help, self.metric.clone());
  }
}

pub struct DockerMetrics {
  pub state_running_boolean: DockerMetric<Family<DockerLabels, Gauge<i64, AtomicI64>>>,
  pub cpu_utilization_percent: DockerMetric<Family<DockerLabels, Gauge<f64, AtomicU64>>>,
  pub memory_usage_bytes: DockerMetric<Family<DockerLabels, Gauge<f64, AtomicU64>>>,
  pub memory_bytes_total: DockerMetric<Family<DockerLabels, Counter<f64, AtomicU64>>>,
  pub memory_utilization_percent: DockerMetric<Family<DockerLabels, Gauge<f64, AtomicU64>>>,
  pub block_io_tx_bytes_total: DockerMetric<Family<DockerLabels, Counter<f64, AtomicU64>>>,
  pub block_io_rx_bytes_total: DockerMetric<Family<DockerLabels, Counter<f64, AtomicU64>>>,
  pub network_tx_bytes_total: DockerMetric<Family<DockerLabels, Counter<f64, AtomicU64>>>,
  pub network_rx_bytes_total: DockerMetric<Family<DockerLabels, Counter<f64, AtomicU64>>>,
}

impl DockerMetrics {
  pub fn new() -> Self {
    Default::default()
  }
}

impl<'a> Default for DockerMetrics {
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
pub struct DockerLabels {
  pub container_id: String,
  pub container_name: String,
}

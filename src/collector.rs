use bollard::{container::Stats, secret::ContainerState, Docker};
use futures::future;
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

use crate::extension::{DockerExt, DockerStatsExt};

pub struct DockerCollector<'a> {
  pub docker: Arc<Docker>,
  pub metrics: Arc<DockerMetrics<'a>>,
}

impl<'a> DockerCollector<'a> {
  pub fn new(docker: Docker, metrics: DockerMetrics<'a>) -> Self {
    Self {
      docker: Arc::new(docker),
      metrics: Arc::new(metrics),
    }
  }

  pub async fn collect(&self) -> Result<Vec<(Option<ContainerState>, Option<Stats>)>, JoinError> {
    let containers = self.docker.get_containers_all().await.unwrap_or_default();

    let result = containers.into_iter().map(|container| {
      let docker = Arc::clone(&self.docker);

      tokio::spawn(async move {
        let container = Arc::new(container);

        let state = {
          let docker = Arc::clone(&docker);
          let container = Arc::clone(&container);
          tokio::spawn(async move { docker.get_container_state(&container.id.as_deref()?).await })
        };

        let stats = {
          let docker = Arc::clone(&docker);
          let container = Arc::clone(&container);
          tokio::spawn(async move { docker.get_container_stats(&container.id.as_deref()?).await })
        };

        let (state, stats) = tokio::join!(state, stats);

        (state.ok().flatten(), stats.ok().flatten())
      })
    });

    future::try_join_all(result).await
  }
}

pub struct DockerMetric<'a, M: Metric + Clone> {
  pub name: &'a str,
  pub help: &'a str,
  pub metric: M,
}

impl<'a, M: Metric + Clone> DockerMetric<'a, M> {
  pub fn new(name: &'a str, help: &'a str, metric: M) -> Self {
    Self { name, help, metric }
  }

  pub fn register(&self, registry: &mut Registry) {
    // Cloning the metric is fine and suggested by the library as it internally uses an Arc
    registry.register(self.name, self.help, self.metric.clone());
  }
}

pub struct DockerMetrics<'a> {
  pub state_running_boolean: DockerMetric<'a, Family<DockerLabels, Gauge<i64, AtomicI64>>>,
  pub cpu_utilization_percent: DockerMetric<'a, Family<DockerLabels, Gauge<f64, AtomicU64>>>,
  pub memory_usage_bytes: DockerMetric<'a, Family<DockerLabels, Gauge<f64, AtomicU64>>>,
  pub memory_bytes_total: DockerMetric<'a, Family<DockerLabels, Counter<f64, AtomicU64>>>,
  pub memory_utilization_percent: DockerMetric<'a, Family<DockerLabels, Gauge<f64, AtomicU64>>>,
  pub block_io_tx_bytes_total: DockerMetric<'a, Family<DockerLabels, Counter<f64, AtomicU64>>>,
  pub block_io_rx_bytes_total: DockerMetric<'a, Family<DockerLabels, Counter<f64, AtomicU64>>>,
  pub network_tx_bytes_total: DockerMetric<'a, Family<DockerLabels, Counter<f64, AtomicU64>>>,
  pub network_rx_bytes_total: DockerMetric<'a, Family<DockerLabels, Counter<f64, AtomicU64>>>,
}

impl<'a> DockerMetrics<'a> {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn process(&self, state: &Option<ContainerState>, stats: &Option<Stats>) {
    let labels = match (
      stats.as_ref().and_then(|s| s.get_id()),
      stats.as_ref().and_then(|s| s.get_name()),
    ) {
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
          .state_running_boolean
          .metric
          .get_or_create(&labels)
          .set(state_running as i64);
      }

      if let Some(true) = state.running {
        if let Some(stats) = stats {
          if let Some(cpu_utilization) = stats.get_cpu_utilization() {
            self
              .cpu_utilization_percent
              .metric
              .get_or_create(&labels)
              .set(cpu_utilization);
          }

          if let Some(memory_usage) = stats.get_memory_usage() {
            self
              .memory_usage_bytes
              .metric
              .get_or_create(&labels)
              .set(memory_usage as f64);
          }

          if let Some(memory_total) = stats.get_memory_total() {
            self
              .memory_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(memory_total as f64);
          }

          if let Some(memory_utilization) = stats.get_memory_utilization() {
            self
              .memory_utilization_percent
              .metric
              .get_or_create(&labels)
              .set(memory_utilization);
          }

          if let Some(block_io_tx_total) = stats.get_block_io_tx_total() {
            self
              .block_io_tx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(block_io_tx_total as f64);
          }

          if let Some(block_io_rx_total) = stats.get_block_io_rx_total() {
            self
              .block_io_rx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(block_io_rx_total as f64);
          }

          if let Some(network_tx_total) = stats.get_network_tx_total() {
            self
              .network_tx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(network_tx_total as f64);
          }

          if let Some(network_rx_total) = stats.get_network_rx_total() {
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

impl<'a> Default for DockerMetrics<'a> {
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

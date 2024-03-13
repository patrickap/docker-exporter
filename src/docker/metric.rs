use bollard::Docker;
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

use crate::docker::container::{self, Container, StatsExt};

pub struct Metric<'a, M: registry::Metric + Clone> {
  name: &'a str,
  help: &'a str,
  metric: M,
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

pub fn init_all<'a>() -> Metrics<'a> {
  Metrics {
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

pub async fn collect_all<'a>(
  docker: Arc<Docker>,
  metrics: Arc<Metrics<'a>>,
) -> Result<(), JoinError> {
  let containers = container::gather_all(docker).await?;

  for container in containers {
    let Container {
      ref state,
      ref stats,
    } = container;

    let labels = match (container.get_id(), container.get_name()) {
      (Some(id), Some(name)) => Some(MetricsLabels {
        container_id: String::from(id),
        container_name: String::from(name),
      }),
      _ => None,
    }
    .unwrap_or_default();

    if let Some(state) = state {
      if let Some(state_running) = state.running {
        metrics
          .state_running_boolean
          .metric
          .get_or_create(&labels)
          .set(state_running as i64);
      }

      if let Some(true) = state.running {
        if let Some(stats) = stats {
          if let Some(cpu_utilization) = stats.get_cpu_utilization() {
            metrics
              .cpu_utilization_percent
              .metric
              .get_or_create(&labels)
              .set(cpu_utilization);
          }

          if let Some(memory_usage) = stats.get_memory_usage() {
            metrics
              .memory_usage_bytes
              .metric
              .get_or_create(&labels)
              .set(memory_usage as f64);
          }

          if let Some(memory_total) = stats.get_memory_total() {
            metrics
              .memory_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(memory_total as f64);
          }

          if let Some(memory_utilization) = stats.get_memory_utilization() {
            metrics
              .memory_utilization_percent
              .metric
              .get_or_create(&labels)
              .set(memory_utilization);
          }

          if let Some(block_io_tx_total) = stats.get_block_io_tx_total() {
            metrics
              .block_io_tx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(block_io_tx_total as f64);
          }

          if let Some(block_io_rx_total) = stats.get_block_io_rx_total() {
            metrics
              .block_io_rx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(block_io_rx_total as f64);
          }

          if let Some(network_tx_total) = stats.get_network_tx_total() {
            metrics
              .network_tx_bytes_total
              .metric
              .get_or_create(&labels)
              .inner()
              .set(network_tx_total as f64);
          }

          if let Some(network_rx_total) = stats.get_network_rx_total() {
            metrics
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

  Ok(())
}

#[derive(Clone, Debug, Default, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct MetricsLabels {
  pub container_id: String,
  pub container_name: String,
}

use bollard::Docker;
use prometheus_client::{
  encoding::EncodeLabelSet,
  metrics::{
    counter::Counter,
    family::Family,
    gauge::{Atomic, Gauge},
  },
  registry::Registry,
};
use std::{
  error::Error,
  sync::{
    atomic::{AtomicI64, AtomicU64},
    Arc,
  },
};

use crate::docker::container::{ContainerInfo, StatsExt};

#[derive(Default)]
pub struct Metrics {
  pub state_running_boolean: Family<MetricsLabels, Gauge<i64, AtomicI64>>,
  pub cpu_utilization_percent: Family<MetricsLabels, Gauge<f64, AtomicU64>>,
  pub memory_usage_bytes: Family<MetricsLabels, Gauge<f64, AtomicU64>>,
  pub memory_bytes_total: Family<MetricsLabels, Counter<f64, AtomicU64>>,
  pub memory_utilization_percent: Family<MetricsLabels, Gauge<f64, AtomicU64>>,
  pub block_io_tx_bytes_total: Family<MetricsLabels, Counter<f64, AtomicU64>>,
  pub block_io_rx_bytes_total: Family<MetricsLabels, Counter<f64, AtomicU64>>,
  pub network_tx_bytes_total: Family<MetricsLabels, Counter<f64, AtomicU64>>,
  pub network_rx_bytes_total: Family<MetricsLabels, Counter<f64, AtomicU64>>,
}

impl Metrics {
  pub fn new() -> Metrics {
    Self {
      ..Default::default()
    }
  }
}

pub trait MetricsRegister<R> {
  fn register_metrics(&self, registry: &mut R) -> ();
}

impl MetricsRegister<Registry> for Metrics {
  fn register_metrics(&self, registry: &mut Registry) {
    registry.register(
      "state_running_boolean",
      "state running as boolean (1 = true, 0 = false)",
      Family::clone(&self.state_running_boolean),
    );

    registry.register(
      "cpu_utilization_percent",
      "cpu utilization in percent",
      Family::clone(&self.cpu_utilization_percent),
    );

    registry.register(
      "memory_usage_bytes",
      "memory usage in bytes",
      Family::clone(&self.memory_usage_bytes),
    );

    registry.register(
      "memory_bytes",
      "memory total in bytes",
      Family::clone(&self.memory_bytes_total),
    );

    registry.register(
      "memory_utilization_percent",
      "memory utilization in percent",
      Family::clone(&self.memory_utilization_percent),
    );

    registry.register(
      "block_io_tx_bytes",
      "block io written total in bytes",
      Family::clone(&self.block_io_tx_bytes_total),
    );

    registry.register(
      "block_io_rx_bytes",
      "block io read total in bytes",
      Family::clone(&self.block_io_rx_bytes_total),
    );

    registry.register(
      "network_tx_bytes",
      "network sent total in bytes",
      Family::clone(&self.network_tx_bytes_total),
    );

    registry.register(
      "network_rx_bytes",
      "network received total in bytes",
      Family::clone(&self.network_rx_bytes_total),
    );
  }
}

pub trait MetricsCollector<S, M> {
  async fn collect_metrics(&self, source: Arc<S>, metrics: Arc<M>) -> Result<(), Box<dyn Error>>;
}

impl MetricsCollector<Docker, Self> for Metrics {
  async fn collect_metrics(
    &self,
    docker: Arc<Docker>,
    metrics: Arc<Metrics>,
  ) -> Result<(), Box<dyn Error>> {
    let containers = ContainerInfo::new(docker).await?;

    for container in containers {
      let ContainerInfo {
        id,
        name,
        state,
        stats,
      } = container;

      let labels = MetricsLabels {
        container_id: id.unwrap_or_default(),
        container_name: name.unwrap_or_default(),
      };

      if let Some(state) = state {
        if let Some(state_running) = state.running {
          metrics
            .state_running_boolean
            .get_or_create(&labels)
            .set(state_running as i64);
        }

        if let Some(true) = state.running {
          if let Some(stats) = stats {
            if let Some(cpu_utilization) = stats.cpu_utilization() {
              metrics
                .cpu_utilization_percent
                .get_or_create(&labels)
                .set(cpu_utilization);
            }

            if let Some(memory_usage) = stats.memory_usage() {
              metrics
                .memory_usage_bytes
                .get_or_create(&labels)
                .set(memory_usage as f64);
            }

            if let Some(memory_total) = stats.memory_total() {
              metrics
                .memory_bytes_total
                .get_or_create(&labels)
                .inner()
                .set(memory_total as f64);
            }

            if let Some(memory_utilization) = stats.memory_utilization() {
              metrics
                .memory_utilization_percent
                .get_or_create(&labels)
                .set(memory_utilization);
            }

            if let Some(block_io_tx_total) = stats.block_io_tx_total() {
              metrics
                .block_io_tx_bytes_total
                .get_or_create(&labels)
                .inner()
                .set(block_io_tx_total as f64);
            }

            if let Some(block_io_rx_total) = stats.block_io_rx_total() {
              metrics
                .block_io_rx_bytes_total
                .get_or_create(&labels)
                .inner()
                .set(block_io_rx_total as f64);
            }

            if let Some(network_tx_total) = stats.network_tx_total() {
              metrics
                .network_tx_bytes_total
                .get_or_create(&labels)
                .inner()
                .set(network_tx_total as f64);
            }

            if let Some(network_rx_total) = stats.network_rx_total() {
              metrics
                .network_rx_bytes_total
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
}

#[derive(Clone, Debug, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct MetricsLabels {
  pub container_id: String,
  pub container_name: String,
}

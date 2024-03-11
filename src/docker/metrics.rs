use prometheus_client::{
  encoding::EncodeLabelSet,
  metrics::{counter::Counter, family::Family, gauge::Gauge},
  registry::Registry,
};
use std::sync::atomic::{AtomicI64, AtomicU64};

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

  pub fn register(&self, registry: &mut Registry) {
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
      "memory_bytes_total",
      "memory total in bytes",
      Family::clone(&self.memory_bytes_total),
    );

    registry.register(
      "memory_utilization_percent",
      "memory utilization in percent",
      Family::clone(&self.memory_utilization_percent),
    );

    registry.register(
      "block_io_tx_bytes_total",
      "block io written total in bytes",
      Family::clone(&self.block_io_tx_bytes_total),
    );

    registry.register(
      "block_io_rx_bytes_total",
      "block io read total in bytes",
      Family::clone(&self.block_io_rx_bytes_total),
    );

    registry.register(
      "network_tx_bytes_total",
      "network sent total in bytes",
      Family::clone(&self.network_tx_bytes_total),
    );

    registry.register(
      "network_rx_bytes_total",
      "network received total in bytes",
      Family::clone(&self.network_rx_bytes_total),
    );
  }
}

#[derive(Clone, Debug, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct MetricsLabels {
  pub container_name: String,
}

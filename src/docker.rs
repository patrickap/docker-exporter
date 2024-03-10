use bollard::{errors::Error, ClientVersion, Docker, API_DEFAULT_VERSION};
use prometheus_client::{
  encoding::EncodeLabelSet,
  metrics::{counter::Counter, family::Family, gauge::Gauge},
  registry::{Metric, Registry},
};
use std::{
  default, env,
  sync::atomic::{AtomicI64, AtomicU64},
};

const HOST_ENV: &str = "DOCKER_HOST";
const DEFAULT_SOCKET_PATH: &str = "/var/run/docker.sock";
const DEFAULT_API_VERSION: &ClientVersion = API_DEFAULT_VERSION;
const DEFAULT_CONNECTION_TIMEOUT: u64 = 60;

pub fn connect() -> Result<Docker, Error> {
  match env::var(HOST_ENV) {
    Ok(docker_host) => Docker::connect_with_http(
      &docker_host,
      DEFAULT_CONNECTION_TIMEOUT,
      DEFAULT_API_VERSION,
    ),
    _ => Docker::connect_with_socket(
      DEFAULT_SOCKET_PATH,
      DEFAULT_CONNECTION_TIMEOUT,
      DEFAULT_API_VERSION,
    ),
  }
}

pub fn create_metrics<'a>() -> ContainerMetrics<'a> {
  ContainerMetrics {
    state_running_boolean: ContainerMetric {
      name: "state_running_boolean",
      help: "state running as boolean (1 = true, 0 = false)",
      metric: Default::default(),
    },
    cpu_utilization_percent: ContainerMetric {
      name: "cpu_utilization_percent",
      help: "cpu utilization in percent",
      metric: Default::default(),
    },
    memory_usage_bytes: ContainerMetric {
      name: "memory_usage_bytes",
      help: "memory usage in bytes",
      metric: Default::default(),
    },
    memory_bytes_total: ContainerMetric {
      name: "memory_bytes_total",
      help: "memory total in bytes",
      metric: Default::default(),
    },
    memory_utilization_percent: ContainerMetric {
      name: "memory_utilization_percent",
      help: "memory utilization in percent",
      metric: Default::default(),
    },
    block_io_tx_bytes_total: ContainerMetric {
      name: "block_io_tx_bytes_total",
      help: "block io written total in bytes",
      metric: Default::default(),
    },
    block_io_rx_bytes_total: ContainerMetric {
      name: "block_io_rx_bytes_total",
      help: "block io read total in bytes",
      metric: Default::default(),
    },
    network_tx_bytes_total: ContainerMetric {
      name: "network_tx_bytes_total",
      help: "network sent total in bytes",
      metric: Default::default(),
    },
    network_rx_bytes_total: ContainerMetric {
      name: "network_rx_bytes_total",
      help: "network received total in bytes",
      metric: Default::default(),
    },
  }
}

// TODO: implement IntoIterator for ContainerMetrics
// pub fn register_metrics<'a, M: Metric + Clone>(
//   registry: &'a mut Registry,
//   metrics: &Vec<ContainerMetric<'a, M>>,
// ) {
//   for ContainerMetric { name, help, metric } in metrics {
//     registry.register(*name, *help, metric.clone());
//   }
// }

pub fn register_metrics(registry: &mut Registry, metrics: &ContainerMetrics) {
  let ContainerMetric { name, help, metric } = &metrics.state_running_boolean;
  registry.register(*name, *help, metric.clone());

  let ContainerMetric { name, help, metric } = &metrics.cpu_utilization_percent;
  registry.register(*name, *help, metric.clone());

  let ContainerMetric { name, help, metric } = &metrics.memory_usage_bytes;
  registry.register(*name, *help, metric.clone());

  let ContainerMetric { name, help, metric } = &metrics.memory_bytes_total;
  registry.register(*name, *help, metric.clone());

  let ContainerMetric { name, help, metric } = &metrics.memory_utilization_percent;
  registry.register(*name, *help, metric.clone());

  let ContainerMetric { name, help, metric } = &metrics.block_io_tx_bytes_total;
  registry.register(*name, *help, metric.clone());

  let ContainerMetric { name, help, metric } = &metrics.block_io_rx_bytes_total;
  registry.register(*name, *help, metric.clone());

  let ContainerMetric { name, help, metric } = &metrics.network_tx_bytes_total;
  registry.register(*name, *help, metric.clone());

  let ContainerMetric { name, help, metric } = &metrics.network_rx_bytes_total;
  registry.register(*name, *help, metric.clone());
}

pub struct ContainerMetric<'a, M: Metric> {
  name: &'a str,
  help: &'a str,
  metric: M,
}

pub struct ContainerMetrics<'a> {
  pub state_running_boolean:
    ContainerMetric<'a, Family<ContainerMetricLabels, Gauge<i64, AtomicI64>>>,
  pub cpu_utilization_percent:
    ContainerMetric<'a, Family<ContainerMetricLabels, Gauge<f64, AtomicU64>>>,
  pub memory_usage_bytes: ContainerMetric<'a, Family<ContainerMetricLabels, Gauge<f64, AtomicU64>>>,
  pub memory_bytes_total:
    ContainerMetric<'a, Family<ContainerMetricLabels, Counter<f64, AtomicU64>>>,
  pub memory_utilization_percent:
    ContainerMetric<'a, Family<ContainerMetricLabels, Gauge<f64, AtomicU64>>>,
  pub block_io_tx_bytes_total:
    ContainerMetric<'a, Family<ContainerMetricLabels, Counter<f64, AtomicU64>>>,
  pub block_io_rx_bytes_total:
    ContainerMetric<'a, Family<ContainerMetricLabels, Counter<f64, AtomicU64>>>,
  pub network_tx_bytes_total:
    ContainerMetric<'a, Family<ContainerMetricLabels, Counter<f64, AtomicU64>>>,
  pub network_rx_bytes_total:
    ContainerMetric<'a, Family<ContainerMetricLabels, Counter<f64, AtomicU64>>>,
}

#[derive(Clone, Debug, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct ContainerMetricLabels {
  container_name: String,
}

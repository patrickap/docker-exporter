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

// TODO: implement IntoIterator for ContainerMetrics
pub fn register_metrics<'a, M: Metric + Clone>(
  registry: &'a mut Registry,
  metrics: &Vec<ContainerMetric<'a, M>>,
) {
  for ContainerMetric { name, help, metric } in metrics {
    registry.register(*name, *help, metric.clone());
  }
}

pub struct ContainerMetric<'a, M: Metric> {
  name: &'a str,
  help: &'a str,
  metric: M,
}

pub struct ContainerMetrics<'a> {
  pub state_running_boolean: ContainerMetric<'a, Family<MetricLabels, Gauge<i64, AtomicI64>>>,
  pub cpu_utilization_percent: ContainerMetric<'a, Family<MetricLabels, Gauge<f64, AtomicU64>>>,
  pub memory_usage_bytes: ContainerMetric<'a, Family<MetricLabels, Gauge<f64, AtomicU64>>>,
  pub memory_bytes_total: ContainerMetric<'a, Family<MetricLabels, Counter<f64, AtomicU64>>>,
  pub memory_utilization_percent: ContainerMetric<'a, Family<MetricLabels, Gauge<f64, AtomicU64>>>,
  pub block_io_tx_bytes_total: ContainerMetric<'a, Family<MetricLabels, Counter<f64, AtomicU64>>>,
  pub block_io_rx_bytes_total: ContainerMetric<'a, Family<MetricLabels, Counter<f64, AtomicU64>>>,
  pub network_tx_bytes_total: ContainerMetric<'a, Family<MetricLabels, Counter<f64, AtomicU64>>>,
  pub network_rx_bytes_total: ContainerMetric<'a, Family<MetricLabels, Counter<f64, AtomicU64>>>,
}

impl<'a> ContainerMetrics<'a> {
  pub fn new() -> ContainerMetrics<'a> {
    Self {
      state_running_boolean: ContainerMetric {
        name: "",
        help: "",
        metric: Default::default(),
      },
      cpu_utilization_percent: ContainerMetric {
        name: "",
        help: "",
        metric: Default::default(),
      },
      memory_usage_bytes: ContainerMetric {
        name: "",
        help: "",
        metric: Default::default(),
      },
      memory_bytes_total: ContainerMetric {
        name: "",
        help: "",
        metric: Default::default(),
      },
      memory_utilization_percent: ContainerMetric {
        name: "",
        help: "",
        metric: Default::default(),
      },
      block_io_tx_bytes_total: ContainerMetric {
        name: "",
        help: "",
        metric: Default::default(),
      },
      block_io_rx_bytes_total: ContainerMetric {
        name: "",
        help: "",
        metric: Default::default(),
      },
      network_tx_bytes_total: ContainerMetric {
        name: "",
        help: "",
        metric: Default::default(),
      },
      network_rx_bytes_total: ContainerMetric {
        name: "",
        help: "",
        metric: Default::default(),
      },
    }
  }
}

#[derive(Clone, Debug, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct MetricLabels {
  container_name: String,
}

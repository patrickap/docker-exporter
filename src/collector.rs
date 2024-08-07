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

impl<M: registry::Metric> Metric<M> {
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
  fn clear(&self);
  fn update(&self, input: Self::Input);
}

pub struct DockerMetrics {
  pub state_running_boolean: Metric<Family<DockerMetricLabels, Gauge<i64, AtomicI64>>>,
  pub cpu_utilization_percent: Metric<Family<DockerMetricLabels, Gauge<f64, AtomicU64>>>,
  pub memory_usage_bytes: Metric<Family<DockerMetricLabels, Gauge<f64, AtomicU64>>>,
  pub memory_limit_bytes: Metric<Family<DockerMetricLabels, Gauge<f64, AtomicU64>>>,
  pub memory_utilization_percent: Metric<Family<DockerMetricLabels, Gauge<f64, AtomicU64>>>,
  pub block_io_tx_bytes_total: Metric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  pub block_io_rx_bytes_total: Metric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  pub network_tx_bytes_total: Metric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
  pub network_rx_bytes_total: Metric<Family<DockerMetricLabels, Counter<f64, AtomicU64>>>,
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
    registry.register_metric(&self.memory_limit_bytes);
    registry.register_metric(&self.memory_utilization_percent);
    registry.register_metric(&self.block_io_tx_bytes_total);
    registry.register_metric(&self.block_io_rx_bytes_total);
    registry.register_metric(&self.network_tx_bytes_total);
    registry.register_metric(&self.network_rx_bytes_total);
  }

  fn clear(&self) {
    self.state_running_boolean.metric.clear();
    self.cpu_utilization_percent.metric.clear();
    self.memory_usage_bytes.metric.clear();
    self.memory_limit_bytes.metric.clear();
    self.memory_utilization_percent.metric.clear();
    self.block_io_tx_bytes_total.metric.clear();
    self.block_io_rx_bytes_total.metric.clear();
    self.network_tx_bytes_total.metric.clear();
    self.network_rx_bytes_total.metric.clear();
  }

  fn update(&self, input: Self::Input) {
    for (state, stats) in input {
      let name = stats.as_ref().and_then(|s| s.name());
      let labels = match name {
        Some(name) => Some(DockerMetricLabels {
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

            if let Some(memory_limit) = stats.memory_limit() {
              self
                .memory_limit_bytes
                .metric
                .get_or_create(&labels)
                .set(memory_limit as f64);
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
      memory_limit_bytes: Metric::new(
        "memory_limit_bytes",
        "memory limit in bytes",
        Default::default(),
      ),
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
  pub container_name: String,
}

#[cfg(test)]
mod tests {
  use bollard::models::ContainerSummary;

  use super::*;
  use crate::extension::DockerExt;

  #[tokio::test]
  async fn it_collects_metrics() {
    struct DockerMock {}

    impl DockerExt for DockerMock {
      async fn list_containers_all(&self) -> Option<Vec<ContainerSummary>> {
        Some(Vec::from([ContainerSummary {
          id: Some(String::from("id_test")),
          ..Default::default()
        }]))
      }
      async fn inspect_container_state(&self, _: &str) -> Option<ContainerState> {
        Some(ContainerState {
          running: Some(true),
          ..Default::default()
        })
      }
      async fn stats_once(&self, _: &str) -> Option<Stats> {
        None
      }
    }

    let docker = DockerMock {};
    let collector = DockerCollector::new(Arc::new(docker));
    let result = collector.collect().await;
    assert_eq!(result.len(), 1);

    let (state, stats) = &result[0];
    assert_eq!(state.as_ref().unwrap().running, Some(true));
    assert_eq!(stats.as_ref(), None)
  }

  #[test]
  fn it_updates_metrics() {
    let labels = DockerMetricLabels {
      ..Default::default()
    };
    let metrics = DockerMetrics::new();

    assert_eq!(
      metrics
        .state_running_boolean
        .metric
        .get_or_create(&labels)
        .get(),
      0
    );

    metrics.update(Vec::from([(
      Some(ContainerState {
        running: Some(true),
        ..Default::default()
      }),
      None,
    )]));

    assert_eq!(
      metrics
        .state_running_boolean
        .metric
        .get_or_create(&labels)
        .get(),
      1
    );
  }
}

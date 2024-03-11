use bollard::{
  container::{
    InspectContainerOptions, ListContainersOptions, MemoryStatsStats, Stats as ContainerStats,
    StatsOptions,
  },
  secret::{ContainerState, ContainerSummary as ContainerList},
  Docker,
};
use futures::{
  future,
  stream::{StreamExt, TryStreamExt},
};
use prometheus_client::metrics::gauge::Atomic;
use std::sync::Arc;
use tokio::task::{self, JoinError};

use super::metrics::{Metrics, MetricsLabels};

pub async fn collect_metrics(docker: Arc<Docker>, metrics: Arc<Metrics>) {
  let infos = ContainerInfo::new(docker).await.unwrap_or_default();

  for info in infos {
    let ContainerInfo {
      id,
      name,
      state,
      stats,
    } = info;

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
}

pub struct ContainerInfo {
  id: Option<String>,
  name: Option<String>,
  state: Option<ContainerState>,
  stats: Option<ContainerStats>,
}

impl ContainerInfo {
  pub async fn new(docker: Arc<Docker>) -> Result<Vec<Self>, JoinError> {
    let containers = Self::get_all(&docker).await.unwrap_or_default();

    let container_infos = containers.into_iter().map(|container| {
      let docker = Arc::clone(&docker);

      task::spawn(async move {
        let container_id = Arc::new(container.id);

        let container_state = {
          let docker = Arc::clone(&docker);
          let container_id = Arc::clone(&container_id);
          task::spawn(async move { Self::get_state(&docker, container_id.as_ref()).await })
        };

        let container_stats = {
          let docker = Arc::clone(&docker);
          let container_id = Arc::clone(&container_id);
          task::spawn(async move { Self::get_stats(&docker, container_id.as_ref()).await })
        };

        let (container_state, container_stats) = tokio::join!(container_state, container_stats);

        Self {
          id: Option::clone(&*container_id),
          name: Self::get_name(container.names),
          state: container_state.ok().flatten(),
          stats: container_stats.ok().flatten(),
        }
      })
    });

    future::try_join_all(container_infos).await
  }

  async fn get_all(docker: &Docker) -> Option<Vec<ContainerList>> {
    docker
      .list_containers(Some(ListContainersOptions::<&str> {
        all: true,
        ..Default::default()
      }))
      .await
      .ok()
  }

  fn get_name(names: Option<Vec<String>>) -> Option<String> {
    names
      .and_then(|names| Some(names.join(";")))
      .and_then(|mut name| Some(name.drain(1..).collect()))
  }

  async fn get_state(docker: &Docker, id: &Option<String>) -> Option<ContainerState> {
    docker
      .inspect_container(
        id.as_deref().unwrap_or_default(),
        Some(InspectContainerOptions {
          ..Default::default()
        }),
      )
      .await
      .ok()
      .and_then(|inspect| inspect.state)
  }

  async fn get_stats(docker: &Docker, id: &Option<String>) -> Option<ContainerStats> {
    docker
      .stats(
        id.as_deref().unwrap_or_default(),
        Some(StatsOptions {
          stream: false,
          ..Default::default()
        }),
      )
      .take(1)
      .try_next()
      .await
      .ok()
      .flatten()
  }
}

pub trait StatsExt {
  fn cpu_delta(&self) -> Option<u64>;
  fn cpu_delta_system(&self) -> Option<u64>;
  fn cpu_count(&self) -> Option<u64>;
  fn cpu_utilization(&self) -> Option<f64>;
  fn memory_usage(&self) -> Option<u64>;
  fn memory_total(&self) -> Option<u64>;
  fn memory_utilization(&self) -> Option<f64>;
  fn block_io_total(&self) -> Option<(u64, u64)>;
  fn block_io_tx_total(&self) -> Option<u64>;
  fn block_io_rx_total(&self) -> Option<u64>;
  fn network_total(&self) -> Option<(u64, u64)>;
  fn network_tx_total(&self) -> Option<u64>;
  fn network_rx_total(&self) -> Option<u64>;
}

impl StatsExt for ContainerStats {
  fn cpu_delta(&self) -> Option<u64> {
    Some(self.cpu_stats.cpu_usage.total_usage - self.precpu_stats.cpu_usage.total_usage)
  }

  fn cpu_delta_system(&self) -> Option<u64> {
    Some(self.cpu_stats.system_cpu_usage? - self.precpu_stats.system_cpu_usage?)
  }

  fn cpu_count(&self) -> Option<u64> {
    self.cpu_stats.online_cpus.or(Some(1))
  }

  fn cpu_utilization(&self) -> Option<f64> {
    Some(
      (self.cpu_delta()? as f64 / self.cpu_delta_system()? as f64)
        * self.cpu_count()? as f64
        * 100.0,
    )
  }

  fn memory_usage(&self) -> Option<u64> {
    let memory_cache = match self.memory_stats.stats? {
      MemoryStatsStats::V1(memory_stats) => memory_stats.cache,
      // In cgroup v2, Docker doesn't provide a cache property
      // Unfortunately, there's no simple way to differentiate cache from memory usage
      MemoryStatsStats::V2(_) => 0,
    };

    Some(self.memory_stats.usage? - memory_cache)
  }

  fn memory_total(&self) -> Option<u64> {
    self.memory_stats.limit
  }

  fn memory_utilization(&self) -> Option<f64> {
    Some((self.memory_usage()? as f64 / self.memory_total()? as f64) * 100.0)
  }

  fn block_io_total(&self) -> Option<(u64, u64)> {
    self
      .blkio_stats
      .io_service_bytes_recursive
      .as_ref()?
      .iter()
      .fold(Some((0, 0)), |acc, io| match io.op.as_str() {
        "write" => Some((acc?.0 + io.value, acc?.1)),
        "read" => Some((acc?.0, acc?.1 + io.value)),
        _ => acc,
      })
  }

  fn block_io_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.block_io_total()?;
    Some(tx)
  }

  fn block_io_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.block_io_total()?;
    Some(rx)
  }

  fn network_total(&self) -> Option<(u64, u64)> {
    let network = self.networks.as_ref()?.get("eth0")?;
    Some((network.tx_bytes, network.rx_bytes))
  }

  fn network_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.network_total()?;
    Some(tx)
  }

  fn network_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.network_total()?;
    Some(rx)
  }
}

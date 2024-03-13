pub mod container;
pub mod metric;

use bollard::{
  container::{
    InspectContainerOptions, ListContainersOptions, MemoryStatsStats, Stats, StatsOptions,
  },
  errors::Error,
  secret::{ContainerState, ContainerSummary},
  Docker,
};
use futures::stream::{StreamExt, TryStreamExt};
use std::env;

use crate::constant::{
  DOCKER_API_VERSION, DOCKER_CONNECTION_TIMEOUT, DOCKER_HOST_ENV, DOCKER_SOCKET_PATH,
};

pub trait DockerExt {
  fn try_connect() -> Result<Docker, Error>;
  async fn list_containers_all(&self) -> Option<Vec<ContainerSummary>>;
  async fn inspect_state(&self, container_name: &str) -> Option<ContainerState>;
  async fn stats_once(&self, container_name: &str) -> Option<Stats>;
  #[cfg(test)]
  fn try_connect_mock() -> Result<Docker, Error>;
}

impl DockerExt for Docker {
  fn try_connect() -> Result<Docker, Error> {
    match env::var(DOCKER_HOST_ENV) {
      Ok(docker_host) => {
        Docker::connect_with_http(&docker_host, DOCKER_CONNECTION_TIMEOUT, DOCKER_API_VERSION)
      }
      _ => Docker::connect_with_socket(
        DOCKER_SOCKET_PATH,
        DOCKER_CONNECTION_TIMEOUT,
        DOCKER_API_VERSION,
      ),
    }
  }

  async fn list_containers_all(&self) -> Option<Vec<ContainerSummary>> {
    let options = Some(ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    });

    self.list_containers(options).await.ok()
  }

  async fn inspect_state(&self, container_name: &str) -> Option<ContainerState> {
    let options = Some(InspectContainerOptions {
      ..Default::default()
    });

    self
      .inspect_container(container_name, options)
      .await
      .ok()
      .and_then(|inspect| inspect.state)
  }

  async fn stats_once(&self, container_name: &str) -> Option<Stats> {
    let options = Some(StatsOptions {
      stream: false,
      ..Default::default()
    });

    self
      .stats(container_name, options)
      .take(1)
      .try_next()
      .await
      .ok()
      .flatten()
  }

  #[cfg(test)]
  fn try_connect_mock() -> Result<Docker, Error> {
    // This is currently sufficient for a test as it returns Ok even if the socket is unavailable
    Docker::connect_with_socket("/dev/null", 0, DOCKER_API_VERSION)
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

impl StatsExt for Stats {
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

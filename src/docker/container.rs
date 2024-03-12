use bollard::{
  container::{
    InspectContainerOptions, ListContainersOptions, MemoryStatsStats, Stats, StatsOptions,
  },
  secret::{ContainerState, ContainerSummary},
  Docker,
};
use futures::{
  future,
  stream::{StreamExt, TryStreamExt},
};
use std::sync::Arc;
use tokio::task::JoinError;

pub struct Container {
  pub id: Option<String>,
  pub name: Option<String>,
  pub state: Option<ContainerState>,
  pub stats: Option<Stats>,
}

pub async fn gather_all(docker: Arc<Docker>) -> Result<Vec<Container>, JoinError> {
  let containers = get_all(&docker).await.unwrap_or(Vec::with_capacity(0));

  let infos = containers.into_iter().map(|container| {
    let docker = Arc::clone(&docker);

    tokio::spawn(async move {
      let container = Arc::new(container);

      let state = {
        let docker = Arc::clone(&docker);
        let container = Arc::clone(&container);
        tokio::spawn(async move { get_state(&docker, &container).await })
      };

      let stats = {
        let docker = Arc::clone(&docker);
        let container = Arc::clone(&container);
        tokio::spawn(async move { get_stats(&docker, &container).await })
      };

      let (state, stats) = tokio::join!(state, stats);

      Container {
        id: container.id.as_ref().map(|id| String::from(id)),
        name: get_name(&container),
        state: state.ok().flatten(),
        stats: stats.ok().flatten(),
      }
    })
  });

  future::try_join_all(infos).await
}

async fn get_all(docker: &Docker) -> Option<Vec<ContainerSummary>> {
  docker
    .list_containers(Some(ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    }))
    .await
    .ok()
}

fn get_name(container: &ContainerSummary) -> Option<String> {
  container
    .names
    .as_ref()
    .and_then(|names| Some(names.join(";")))
    .and_then(|mut name| Some(name.drain(1..).collect()))
}

async fn get_state(docker: &Docker, container: &ContainerSummary) -> Option<ContainerState> {
  docker
    .inspect_container(
      container.id.as_deref().unwrap_or(""),
      Some(InspectContainerOptions {
        ..Default::default()
      }),
    )
    .await
    .ok()
    .and_then(|inspect| inspect.state)
}

async fn get_stats(docker: &Docker, container: &ContainerSummary) -> Option<Stats> {
  docker
    .stats(
      container.id.as_deref().unwrap_or(""),
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

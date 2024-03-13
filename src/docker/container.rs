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

pub async fn retrieve(docker: Arc<Docker>) -> Result<Vec<Container>, JoinError> {
  let summaries = get_summary(&docker).await.unwrap_or_default();

  let result = summaries.into_iter().map(|summary| {
    let docker = Arc::clone(&docker);

    tokio::spawn(async move {
      let summary = Arc::new(summary);

      let state = {
        let docker = Arc::clone(&docker);
        let summary = Arc::clone(&summary);
        tokio::spawn(async move { get_state(&docker, &summary.id.as_deref()?).await })
      };

      let stats = {
        let docker = Arc::clone(&docker);
        let summary = Arc::clone(&summary);
        tokio::spawn(async move { get_stats(&docker, &summary.id.as_deref()?).await })
      };

      let (state, stats) = tokio::join!(state, stats);
      let (state, stats) = (state.ok().flatten(), stats.ok().flatten());

      Container {
        id: stats.as_ref().map(|s| String::from(&s.id)),
        name: stats.as_ref().map(|s| String::from(&s.name[1..])),
        state,
        stats,
      }
    })
  });

  future::try_join_all(result).await
}

async fn get_summary(docker: &Docker) -> Option<Vec<ContainerSummary>> {
  docker
    .list_containers(Some(ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    }))
    .await
    .ok()
}

async fn get_state(docker: &Docker, id: &str) -> Option<ContainerState> {
  docker
    .inspect_container(
      id,
      Some(InspectContainerOptions {
        ..Default::default()
      }),
    )
    .await
    .ok()
    .and_then(|inspect| inspect.state)
}

async fn get_stats(docker: &Docker, id: &str) -> Option<Stats> {
  docker
    .stats(
      id,
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
  fn get_cpu_delta(&self) -> Option<u64>;
  fn get_cpu_delta_system(&self) -> Option<u64>;
  fn get_cpu_count(&self) -> Option<u64>;
  fn get_cpu_utilization(&self) -> Option<f64>;
  fn get_memory_usage(&self) -> Option<u64>;
  fn get_memory_total(&self) -> Option<u64>;
  fn get_memory_utilization(&self) -> Option<f64>;
  fn get_block_io_total(&self) -> Option<(u64, u64)>;
  fn get_block_io_tx_total(&self) -> Option<u64>;
  fn get_block_io_rx_total(&self) -> Option<u64>;
  fn get_network_total(&self) -> Option<(u64, u64)>;
  fn get_network_tx_total(&self) -> Option<u64>;
  fn get_network_rx_total(&self) -> Option<u64>;
}

impl StatsExt for Stats {
  fn get_cpu_delta(&self) -> Option<u64> {
    Some(self.cpu_stats.cpu_usage.total_usage - self.precpu_stats.cpu_usage.total_usage)
  }

  fn get_cpu_delta_system(&self) -> Option<u64> {
    Some(self.cpu_stats.system_cpu_usage? - self.precpu_stats.system_cpu_usage?)
  }

  fn get_cpu_count(&self) -> Option<u64> {
    self.cpu_stats.online_cpus.or(Some(1))
  }

  fn get_cpu_utilization(&self) -> Option<f64> {
    Some(
      (self.get_cpu_delta()? as f64 / self.get_cpu_delta_system()? as f64)
        * self.get_cpu_count()? as f64
        * 100.0,
    )
  }

  fn get_memory_usage(&self) -> Option<u64> {
    let memory_cache = match self.memory_stats.stats? {
      MemoryStatsStats::V1(memory_stats) => memory_stats.cache,
      // In cgroup v2, Docker doesn't provide a cache property
      // Unfortunately, there's no simple way to differentiate cache from memory usage
      MemoryStatsStats::V2(_) => 0,
    };

    Some(self.memory_stats.usage? - memory_cache)
  }

  fn get_memory_total(&self) -> Option<u64> {
    self.memory_stats.limit
  }

  fn get_memory_utilization(&self) -> Option<f64> {
    Some((self.get_memory_usage()? as f64 / self.get_memory_total()? as f64) * 100.0)
  }

  fn get_block_io_total(&self) -> Option<(u64, u64)> {
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

  fn get_block_io_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.get_block_io_total()?;
    Some(tx)
  }

  fn get_block_io_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.get_block_io_total()?;
    Some(rx)
  }

  fn get_network_total(&self) -> Option<(u64, u64)> {
    let network = self.networks.as_ref()?.get("eth0")?;
    Some((network.tx_bytes, network.rx_bytes))
  }

  fn get_network_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.get_network_total()?;
    Some(tx)
  }

  fn get_network_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.get_network_total()?;
    Some(rx)
  }
}

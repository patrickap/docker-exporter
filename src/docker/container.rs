use bollard::{
  container::{
    InspectContainerOptions, ListContainersOptions, MemoryStatsStats, Stats, StatsOptions,
  },
  Docker,
};
use futures::{
  stream::{StreamExt, TryStreamExt},
  FutureExt,
};
use prometheus_client::metrics::gauge::Atomic;
use std::sync::Arc;
use tokio::task::{self, JoinSet};

use super::metrics::{Metrics, MetricsLabels};

pub async fn collect_metrics(docker: Arc<Docker>, metrics: Arc<Metrics>) {
  let containers = docker
    .list_containers(Some(ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    }))
    .await
    .unwrap_or_default();

  let mut main_set = JoinSet::new();

  for container in containers {
    let docker = Arc::clone(&docker);
    let metrics = Arc::clone(&metrics);
    let mut set = JoinSet::new();

    main_set.spawn(async move {
      let id = Arc::new(container.id.unwrap_or_default());

      let name = Arc::new(
        container
          .names
          .and_then(|names| Some(names.join(";")))
          .and_then(|mut name| Some(name.drain(1..).collect::<String>()))
          .unwrap_or_default(),
      );

      let state = {
        let docker = Arc::clone(&docker);
        let id = Arc::clone(&id);

        task::spawn(async move {
          docker
            .inspect_container(
              &id,
              Some(InspectContainerOptions {
                ..Default::default()
              }),
            )
            .await
        })
        .map(|inspect| match inspect {
          Ok(Ok(inspect)) => Arc::new(inspect.state),
          _ => Arc::new(None),
        })
      };

      let stats = {
        let docker = Arc::clone(&docker);
        let id = Arc::clone(&id);

        task::spawn(async move {
          docker
            .stats(
              &id,
              Some(StatsOptions {
                stream: false,
                ..Default::default()
              }),
            )
            .take(1)
            .try_next()
            .await
        })
        .map(|stats| match stats {
          Ok(Ok(stats)) => Arc::new(stats),
          _ => Arc::new(None),
        })
      };

      let (state, stats) = tokio::join!(state, stats);
      let labels = Arc::new(MetricsLabels {
        container_name: String::from(&*name),
      });

      {
        let metrics = Arc::clone(&metrics);
        let state = Arc::clone(&state);
        let labels = Arc::clone(&labels);
        set.spawn(async move {
          let metric = Option::as_ref(&state).and_then(|s| s.running);
          metrics
            .state_running_boolean
            .get_or_create(&labels)
            .set(metric.unwrap_or_default() as i64);
        });
      }

      let running = Option::as_ref(&state)
        .and_then(|s| s.running)
        .unwrap_or_default();

      if running {
        {
          let metrics = Arc::clone(&metrics);
          let stats = Arc::clone(&stats);
          let labels = Arc::clone(&labels);
          set.spawn(async move {
            let metric = Option::as_ref(&stats).and_then(|s| s.cpu_utilization());
            if let Some(metric) = metric {
              metrics
                .cpu_utilization_percent
                .get_or_create(&labels)
                .set(metric);
            }
          });
        }

        {
          let metrics = Arc::clone(&metrics);
          let stats = Arc::clone(&stats);
          let labels = Arc::clone(&labels);
          set.spawn(async move {
            let metric = Option::as_ref(&stats).and_then(|s| s.memory_usage());
            if let Some(metric) = metric {
              metrics
                .memory_usage_bytes
                .get_or_create(&labels)
                .inner()
                .set(metric as f64);
            }
          });
        }

        {
          let metrics = Arc::clone(&metrics);
          let stats = Arc::clone(&stats);
          let labels = Arc::clone(&labels);
          set.spawn(async move {
            let metric = Option::as_ref(&stats).and_then(|s| s.memory_total());
            if let Some(metric) = metric {
              metrics
                .memory_bytes_total
                .get_or_create(&labels)
                .inner()
                .set(metric as f64);
            }
          });
        }

        {
          let metrics = Arc::clone(&metrics);
          let stats = Arc::clone(&stats);
          let labels = Arc::clone(&labels);
          set.spawn(async move {
            let metric = Option::as_ref(&stats).and_then(|s| s.memory_utilization());
            if let Some(metric) = metric {
              metrics
                .memory_utilization_percent
                .get_or_create(&labels)
                .inner()
                .set(metric);
            }
          });
        }

        {
          let metrics = Arc::clone(&metrics);
          let stats = Arc::clone(&stats);
          let labels = Arc::clone(&labels);
          set.spawn(async move {
            let metric = Option::as_ref(&stats).and_then(|s| s.block_io_tx_total());
            if let Some(metric) = metric {
              metrics
                .block_io_tx_bytes_total
                .get_or_create(&labels)
                .inner()
                .set(metric as f64);
            }
          });
        }

        {
          let metrics = Arc::clone(&metrics);
          let stats = Arc::clone(&stats);
          let labels = Arc::clone(&labels);
          set.spawn(async move {
            let metric = Option::as_ref(&stats).and_then(|s| s.block_io_rx_total());
            if let Some(metric) = metric {
              metrics
                .block_io_rx_bytes_total
                .get_or_create(&labels)
                .inner()
                .set(metric as f64);
            }
          });
        }

        {
          let metrics = Arc::clone(&metrics);
          let stats = Arc::clone(&stats);
          let labels = Arc::clone(&labels);
          set.spawn(async move {
            let metric = Option::as_ref(&stats).and_then(|s| s.network_tx_total());
            if let Some(metric) = metric {
              metrics
                .network_tx_bytes_total
                .get_or_create(&labels)
                .inner()
                .set(metric as f64);
            }
          });
        }

        {
          let metrics = Arc::clone(&metrics);
          let stats = Arc::clone(&stats);
          let labels = Arc::clone(&labels);
          set.spawn(async move {
            let metric = Option::as_ref(&stats).and_then(|s| s.network_rx_total());
            if let Some(metric) = metric {
              metrics
                .network_rx_bytes_total
                .get_or_create(&labels)
                .inner()
                .set(metric as f64);
            }
          });
        }
      }

      while let Some(_) = set.join_next().await {}
    });
  }

  while let Some(_) = main_set.join_next().await {}
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

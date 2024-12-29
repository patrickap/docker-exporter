use bollard::{
  container::{MemoryStatsStats, Stats},
  secret::{ContainerState, HealthStatusEnum},
  Docker,
};
use futures::future::{self};
use prometheus_client::{
  collector::Collector,
  encoding::{DescriptorEncoder, EncodeLabelSet, EncodeMetric},
  metrics::{counter::ConstCounter, gauge::ConstGauge},
};
use std::{error::Error, sync::Arc};
use tokio::{runtime::Handle, task};

use crate::extension::DockerExt;

#[derive(Debug)]
pub struct DockerCollector {
  docker: Arc<Docker>,
}

impl DockerCollector {
  pub fn new(docker: Docker) -> Self {
    return Self {
      docker: Arc::new(docker),
    };
  }

  // TODO: move labels declaration to correct place for each container
  async fn collect<'a>(&self) -> Result<Vec<DockerMetric<'a>>, Box<dyn Error>> {
    let docker = Arc::clone(&self.docker);
    let containers = docker.list_containers_all().await.unwrap_or_default();

    let tasks = containers.into_iter().map(|container| {
      let docker = Arc::clone(&docker);

      tokio::spawn(async move {
        let name = container.id.as_deref().unwrap_or_default();
        let state = docker.inspect_container_state(&name);
        let stats = docker.stats_once(&name);
        tokio::join!(state, stats)
      })
    });

    let metrics = future::try_join_all(tasks)
      .await?
      .into_iter()
      .flat_map(|(state, stats)| {
        Vec::from(
          [
            self.state_metrics(state.as_ref()).unwrap_or_default(),
            self.cpu_metrics(stats.as_ref()).unwrap_or_default(),
            self.memory_metrics(stats.as_ref()).unwrap_or_default(),
            self.block_metrics(stats.as_ref()).unwrap_or_default(),
            self.network_metrics(stats.as_ref()).unwrap_or_default(),
          ]
          .into_iter()
          .fold(Vec::new(), |mut acc, mut curr| {
            acc.append(&mut curr);
            acc
          }),
        )
      })
      .collect();

    Ok(metrics)
  }

  fn state_metrics<'a>(&self, state: Option<&ContainerState>) -> Option<Vec<DockerMetric<'a>>> {
    let running = state.and_then(|s| s.running).unwrap_or_default() as i64;

    let healthy = state
      .and_then(|s| s.health.as_ref())
      .map(|h| (h.status == Some(HealthStatusEnum::HEALTHY)))
      .unwrap_or_default() as i64;

    Some(Vec::from([
      DockerMetric::new(
        "state_running_boolean",
        "state running as boolean (1 = true, 0 = false)",
        ConstGauge::new(running),
      ),
      DockerMetric::new(
        "state_healthy_boolean",
        "state healthy as boolean (1 = true, 0 = false)",
        ConstGauge::new(healthy),
      ),
    ]))
  }

  fn cpu_metrics<'a>(&self, stats: Option<&Stats>) -> Option<Vec<DockerMetric<'a>>> {
    let cpu_delta =
      stats?.cpu_stats.cpu_usage.total_usage - stats?.precpu_stats.cpu_usage.total_usage;

    let cpu_delta_system =
      stats?.cpu_stats.system_cpu_usage? - stats?.precpu_stats.system_cpu_usage?;

    let cpu_count = stats?.cpu_stats.online_cpus?;

    let cpu_utilization = (cpu_delta as f64 / cpu_delta_system as f64) * cpu_count as f64 * 100.0;

    Some(Vec::from([DockerMetric::new(
      "cpu_utilization_percent",
      "cpu utilization in percent",
      ConstGauge::new(cpu_utilization),
    )]))
  }

  fn memory_metrics<'a>(&self, stats: Option<&Stats>) -> Option<Vec<DockerMetric<'a>>> {
    let memory_usage = match stats?.memory_stats.stats? {
      MemoryStatsStats::V1(memory_stats) => stats?.memory_stats.usage? - memory_stats.cache,
      // In cgroup v2, Docker doesn't provide a cache property
      // Unfortunately, there's no simple way to differentiate cache from memory usage
      MemoryStatsStats::V2(_) => stats?.memory_stats.usage?,
    };

    let memory_limit = stats?.memory_stats.limit?;

    let memory_utilization = (memory_usage as f64 / memory_limit as f64) * 100.0;

    Some(Vec::from([
      DockerMetric::new(
        "memory_usage_bytes",
        "memory usage in bytes",
        ConstGauge::new(memory_usage as f64),
      ),
      DockerMetric::new(
        "memory_limit_bytes",
        "memory limit in bytes",
        ConstGauge::new(memory_limit as f64),
      ),
      DockerMetric::new(
        "memory_utilization_percent",
        "memory utilization in percent",
        ConstGauge::new(memory_utilization),
      ),
    ]))
  }

  fn block_metrics<'a>(&self, stats: Option<&Stats>) -> Option<Vec<DockerMetric<'a>>> {
    let (block_io_tx, block_io_rx) = stats?
      .blkio_stats
      .io_service_bytes_recursive
      .as_ref()?
      .iter()
      .fold(Some((0, 0)), |acc, io| match io.op.as_str() {
        "write" => Some((acc?.0 + io.value, acc?.1)),
        "read" => Some((acc?.0, acc?.1 + io.value)),
        _ => acc,
      })?;

    Some(Vec::from([
      DockerMetric::new(
        "block_io_tx_bytes",
        "block io written total in bytes",
        ConstCounter::new(block_io_tx),
      ),
      DockerMetric::new(
        "block_io_rx_bytes",
        "block io read total in bytes",
        ConstCounter::new(block_io_rx),
      ),
    ]))
  }

  fn network_metrics<'a>(&self, stats: Option<&Stats>) -> Option<Vec<DockerMetric<'a>>> {
    let (network_tx, network_rx) = stats?
      .networks
      .as_ref()?
      .get("eth0")
      .map(|n| (n.tx_bytes, n.rx_bytes))?;

    Some(Vec::from([
      DockerMetric::new(
        "network_tx_bytes",
        "network sent total in bytes",
        ConstCounter::new(network_tx),
      ),
      DockerMetric::new(
        "network_rx_bytes",
        "network received total in bytes",
        ConstCounter::new(network_rx),
      ),
    ]))
  }
}

// TODO: do not unwrap if possible
impl Collector for DockerCollector {
  fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
    // Blocking is required as encode is not available as async function.
    task::block_in_place(move || {
      // Reentering the async context for gathering the metrics.
      Handle::current()
        .block_on(async move {
          let labels = DockerMetricLabels {
            container_name: "container_name".to_string(),
          };

          let metrics = self.collect().await.unwrap();

          metrics.iter().for_each(|metric| {
            let mut metric_encoder = encoder
              .encode_descriptor(metric.name, metric.help, None, metric.metric.metric_type())
              .unwrap();
            let metric_encoder = metric_encoder.encode_family(&labels).unwrap();

            metric.metric.encode(metric_encoder).unwrap();
          });

          Ok::<(), Box<dyn std::error::Error>>(())
        })
        .unwrap()
    });

    Ok(())
  }
}

pub struct DockerMetric<'a> {
  name: &'a str,
  help: &'a str,
  metric: Box<dyn EncodeMetric + 'a>,
}

impl<'a> DockerMetric<'a> {
  fn new(name: &'a str, help: &'a str, metric: impl EncodeMetric + 'a) -> Self {
    Self {
      name,
      help,
      metric: Box::new(metric),
    }
  }
}

#[derive(EncodeLabelSet)]
pub struct DockerMetricLabels {
  pub container_name: String,
}

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
use std::{error::Error, rc::Rc, sync::Arc};
use tokio::{runtime::Handle, task};

use crate::extension::DockerExt;

// TODO: do not unwrap if possible. when use ? vs unwrap? check all
// TODO: move metric methods outside collector as they do not need self or make them static
// TODO: maybe move encoder processing outside method

#[derive(Debug)]
pub struct DockerCollector {
  docker: Arc<Docker>,
}

impl DockerCollector {
  pub fn new(docker: Arc<Docker>) -> Self {
    return Self { docker };
  }

  pub async fn recv_metrics<'a>(&self) -> Result<Vec<DockerMetric<'a>>, Box<dyn Error>> {
    let docker = Arc::clone(&self.docker);
    let containers = docker.list_containers_all().await.unwrap_or_default();

    let tasks = containers.into_iter().map(|container| {
      let docker = Arc::clone(&docker);

      tokio::spawn(async move {
        let id = container.id.as_deref().unwrap_or_default();
        let state = docker.inspect_container_state(&id);
        let stats = docker.stats_once(&id);
        tokio::join!(state, stats)
      })
    });

    let metrics = future::try_join_all(tasks)
      .await?
      .into_iter()
      .flat_map(|(state, stats)| {
        let args = (state.as_ref(), stats.as_ref());

        Vec::from(
          [
            Self::state_metrics(args),
            Self::cpu_metrics(args),
            Self::memory_metrics(args),
            Self::block_metrics(args),
            Self::network_metrics(args),
          ]
          .into_iter()
          .fold(Vec::new(), |mut acc, curr| {
            acc.append(&mut curr.unwrap_or_default());
            acc
          }),
        )
      })
      .collect();

    Ok(metrics)
  }

  fn state_metrics<'a>(
    (state, stats): (Option<&ContainerState>, Option<&Stats>),
  ) -> Option<Vec<DockerMetric<'a>>> {
    let running = state?.running? as i64;

    let healthy = state?
      .health
      .as_ref()
      .map(|h| (h.status == Some(HealthStatusEnum::HEALTHY)))? as i64;

    let labels = Rc::new(DockerMetricLabels {
      container_name: stats?.name[1..].to_string(),
    });

    Some(Vec::from([
      DockerMetric::new(
        "state_running_boolean",
        "state running as boolean (1 = true, 0 = false)",
        Box::new(ConstGauge::new(running)),
        Rc::clone(&labels),
      ),
      DockerMetric::new(
        "state_healthy_boolean",
        "state healthy as boolean (1 = true, 0 = false)",
        Box::new(ConstGauge::new(healthy)),
        Rc::clone(&labels),
      ),
    ]))
  }

  fn cpu_metrics<'a>(
    (_, stats): (Option<&ContainerState>, Option<&Stats>),
  ) -> Option<Vec<DockerMetric<'a>>> {
    let cpu_delta =
      stats?.cpu_stats.cpu_usage.total_usage - stats?.precpu_stats.cpu_usage.total_usage;

    let cpu_delta_system =
      stats?.cpu_stats.system_cpu_usage? - stats?.precpu_stats.system_cpu_usage?;

    let cpu_count = stats?.cpu_stats.online_cpus?;

    let cpu_utilization = (cpu_delta as f64 / cpu_delta_system as f64) * cpu_count as f64 * 100.0;

    let labels = Rc::new(DockerMetricLabels {
      container_name: stats?.name[1..].to_string(),
    });

    Some(Vec::from([DockerMetric::new(
      "cpu_utilization_percent",
      "cpu utilization in percent",
      Box::new(ConstGauge::new(cpu_utilization)),
      Rc::clone(&labels),
    )]))
  }

  fn memory_metrics<'a>(
    (_, stats): (Option<&ContainerState>, Option<&Stats>),
  ) -> Option<Vec<DockerMetric<'a>>> {
    let memory_usage = match stats?.memory_stats.stats? {
      MemoryStatsStats::V1(memory_stats) => stats?.memory_stats.usage? - memory_stats.cache,
      // In cgroup v2, Docker doesn't provide a cache property
      // Unfortunately, there's no simple way to differentiate cache from memory usage
      MemoryStatsStats::V2(_) => stats?.memory_stats.usage?,
    };

    let memory_limit = stats?.memory_stats.limit?;

    let memory_utilization = (memory_usage as f64 / memory_limit as f64) * 100.0;

    let labels = Rc::new(DockerMetricLabels {
      container_name: stats?.name[1..].to_string(),
    });

    Some(Vec::from([
      DockerMetric::new(
        "memory_usage_bytes",
        "memory usage in bytes",
        Box::new(ConstGauge::new(memory_usage as f64)),
        Rc::clone(&labels),
      ),
      DockerMetric::new(
        "memory_limit_bytes",
        "memory limit in bytes",
        Box::new(ConstGauge::new(memory_limit as f64)),
        Rc::clone(&labels),
      ),
      DockerMetric::new(
        "memory_utilization_percent",
        "memory utilization in percent",
        Box::new(ConstGauge::new(memory_utilization)),
        Rc::clone(&labels),
      ),
    ]))
  }

  fn block_metrics<'a>(
    (_, stats): (Option<&ContainerState>, Option<&Stats>),
  ) -> Option<Vec<DockerMetric<'a>>> {
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

    let labels = Rc::new(DockerMetricLabels {
      container_name: stats?.name[1..].to_string(),
    });

    Some(Vec::from([
      DockerMetric::new(
        "block_io_tx_bytes",
        "block io written total in bytes",
        Box::new(ConstCounter::new(block_io_tx)),
        Rc::clone(&labels),
      ),
      DockerMetric::new(
        "block_io_rx_bytes",
        "block io read total in bytes",
        Box::new(ConstCounter::new(block_io_rx)),
        Rc::clone(&labels),
      ),
    ]))
  }

  fn network_metrics<'a>(
    (_, stats): (Option<&ContainerState>, Option<&Stats>),
  ) -> Option<Vec<DockerMetric<'a>>> {
    let (network_tx, network_rx) = stats?
      .networks
      .as_ref()?
      .get("eth0")
      .map(|n| (n.tx_bytes, n.rx_bytes))?;

    let labels = Rc::new(DockerMetricLabels {
      container_name: stats?.name[1..].to_string(),
    });

    Some(Vec::from([
      DockerMetric::new(
        "network_tx_bytes",
        "network sent total in bytes",
        Box::new(ConstCounter::new(network_tx)),
        Rc::clone(&labels),
      ),
      DockerMetric::new(
        "network_rx_bytes",
        "network received total in bytes",
        Box::new(ConstCounter::new(network_rx)),
        Rc::clone(&labels),
      ),
    ]))
  }
}

impl Collector for DockerCollector {
  fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
    // Blocking is required as encode is not available as async function.
    task::block_in_place(move || {
      // Reentering the async context for gathering the metrics.
      Handle::current()
        .block_on(async move {
          let metrics = self.recv_metrics().await.unwrap_or_default();

          metrics.iter().for_each(|metric| {
            encoder
              .encode_descriptor(metric.name, metric.help, None, metric.metric.metric_type())
              .and_then(|mut e| {
                e.encode_family(metric.labels.as_ref())?;
                Ok(e)
              })
              .and_then(|e| {
                metric.metric.encode(e)?;
                Ok(())
              })
              .unwrap_or_default();
          });

          Ok::<(), Box<dyn std::error::Error>>(())
        })
        .unwrap_or_default()
    });

    Ok(())
  }
}

pub struct DockerMetric<'a> {
  name: &'a str,
  help: &'a str,
  metric: Box<dyn EncodeMetric + 'a>,
  labels: Rc<DockerMetricLabels>,
}

impl<'a> DockerMetric<'a> {
  fn new(
    name: &'a str,
    help: &'a str,
    metric: Box<dyn EncodeMetric + 'a>,
    labels: Rc<DockerMetricLabels>,
  ) -> Self {
    Self {
      name,
      help,
      metric,
      labels,
    }
  }
}

#[derive(EncodeLabelSet)]
pub struct DockerMetricLabels {
  pub container_name: String,
}

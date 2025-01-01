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

#[derive(Debug)]
pub struct DockerCollector {
  docker: Arc<Docker>,
}

impl DockerCollector {
  pub fn new(docker: Arc<Docker>) -> Self {
    return Self { docker };
  }

  pub async fn collect<'a>(&self) -> Result<Vec<DockerMetric<'a>>, Box<dyn Error>> {
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
    let running = state.and_then(|s| s.running).unwrap_or_default();

    let healthy = state
      .and_then(|s| s.health.as_ref())
      .map(|h| (h.status == Some(HealthStatusEnum::HEALTHY)))
      .unwrap_or_default();

    let labels = Rc::new(DockerMetricLabels {
      container_name: stats
        .map(|s| String::from(&s.name[1..]))
        .unwrap_or_default(),
    });

    Some(Vec::from([
      DockerMetric::new(
        "state_running_boolean",
        "state running as boolean (1 = true, 0 = false)",
        Box::new(ConstGauge::new(running as i64)),
        Rc::clone(&labels),
      ),
      DockerMetric::new(
        "state_healthy_boolean",
        "state healthy as boolean (1 = true, 0 = false)",
        Box::new(ConstGauge::new(healthy as i64)),
        Rc::clone(&labels),
      ),
    ]))
  }

  fn cpu_metrics<'a>(
    (_, stats): (Option<&ContainerState>, Option<&Stats>),
  ) -> Option<Vec<DockerMetric<'a>>> {
    let cpu_delta = stats
      .map(|s| s.cpu_stats.cpu_usage.total_usage - s.precpu_stats.cpu_usage.total_usage)
      .unwrap_or_default();

    let cpu_delta_system = stats
      .and_then(|s| {
        s.cpu_stats
          .system_cpu_usage
          .zip(s.precpu_stats.system_cpu_usage)
      })
      .map(|(cpu_usage, precpu_usage)| cpu_usage - precpu_usage)
      .unwrap_or_default();

    let cpu_count = stats
      .and_then(|s| s.cpu_stats.online_cpus)
      .unwrap_or_default();

    let cpu_utilization = (cpu_delta as f64 / cpu_delta_system as f64) * cpu_count as f64 * 100.0;

    let labels = Rc::new(DockerMetricLabels {
      container_name: stats
        .map(|s| String::from(&s.name[1..]))
        .unwrap_or_default(),
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
    let memory_usage = stats
      .map(|s| s.memory_stats)
      .and_then(|m| match m.stats {
        Some(MemoryStatsStats::V1(memory_stats)) => m.usage.map(|u| u - memory_stats.cache),
        // In cgroup v2, Docker doesn't provide a cache property.
        // Unfortunately, there's no simple way to differentiate cache from memory usage.
        Some(MemoryStatsStats::V2(_)) => m.usage,
        None => None,
      })
      .unwrap_or_default();

    let memory_limit = stats.and_then(|s| s.memory_stats.limit).unwrap_or_default();

    let memory_utilization = (memory_usage as f64 / memory_limit as f64) * 100.0;

    let labels = Rc::new(DockerMetricLabels {
      container_name: stats
        .map(|s| String::from(&s.name[1..]))
        .unwrap_or_default(),
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
    let (block_io_tx, block_io_rx) = stats
      .and_then(|s| s.blkio_stats.io_service_bytes_recursive.as_ref())
      .map(|io| {
        io.iter().fold((0, 0), |acc, io| match io.op.as_str() {
          "write" => (acc.0 + io.value, acc.1),
          "read" => (acc.0, acc.1 + io.value),
          _ => acc,
        })
      })
      .unwrap_or_default();

    let labels = Rc::new(DockerMetricLabels {
      container_name: stats
        .map(|s| String::from(&s.name[1..]))
        .unwrap_or_default(),
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
    let (network_tx, network_rx) = stats
      .and_then(|s| s.networks.as_ref())
      .and_then(|n| n.get("eth0"))
      .map(|n| (n.tx_bytes, n.rx_bytes))
      .unwrap_or_default();

    let labels = Rc::new(DockerMetricLabels {
      container_name: stats
        .map(|s| String::from(&s.name[1..]))
        .unwrap_or_default(),
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
    // Blocking is necessary because the `encode`` implementation is synchronous.
    task::block_in_place(move || {
      // Reentering the async context to collect the metrics.
      Handle::current()
        .block_on(async move {
          let metrics = self.collect().await.unwrap_or_default();

          metrics.iter().for_each(|metric| {
            if let Err(e) = metric.encode(&mut encoder) {
              eprintln!("Error processing metric: {}", e);
            }
          });

          Ok::<(), Box<dyn Error>>(())
        })
        .unwrap_or_default();
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
  pub fn new(
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

  fn encode(&self, encoder: &mut DescriptorEncoder) -> Result<(), Box<dyn std::error::Error>> {
    let mut metric_encoder = encoder.encode_descriptor(
      self.name,
      self.help,
      None,
      self.metric.as_ref().metric_type(),
    )?;

    let metric_encoder = metric_encoder.encode_family(self.labels.as_ref())?;

    self.metric.as_ref().encode(metric_encoder)?;

    Ok(())
  }
}

#[derive(EncodeLabelSet, Debug)]
pub struct DockerMetricLabels {
  pub container_name: String,
}

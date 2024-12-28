use bollard::{
  secret::{ContainerState, HealthStatusEnum},
  Docker,
};
use futures::future::{self};
use prometheus_client::{
  collector::Collector,
  encoding::{DescriptorEncoder, EncodeLabelSet, EncodeMetric},
  metrics::gauge::ConstGauge,
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
            self.state_metrics(state.as_ref()),
            // self.cpu_metrics(stats.as_ref()),
            // self.memory_metrics(stats.as_ref()),
            // self.block_metrics(stats.as_ref()),
            // self.network_metrics(stats.as_ref()),
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

  fn state_metrics<'a>(&self, state: Option<&ContainerState>) -> Vec<DockerMetric<'a>> {
    let running = state.and_then(|s| s.running).unwrap_or_default() as i64;
    let healthy = state
      .and_then(|s| s.health.as_ref())
      .map(|h| (h.status == Some(HealthStatusEnum::HEALTHY)))
      .unwrap_or_default() as i64;

    Vec::from([
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
    ])
  }

  // fn cpu_metrics<'a>(&self, stats: Option<&Stats>) -> Vec<DockerMetric<'a>> {}

  // fn memory_metrics<'a>(&self, stats: Option<&Stats>) -> Vec<DockerMetric<'a>> {}

  // fn block_metrics<'a>(&self, stats: Option<&Stats>) -> Vec<DockerMetric<'a>> {}

  // fn network_metrics<'a>(&self, stats: Option<&Stats>) -> Vec<DockerMetric<'a>> {}
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

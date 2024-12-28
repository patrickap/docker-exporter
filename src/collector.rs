use bollard::Docker;
use futures::future::{self};
use prometheus_client::{
  collector::Collector,
  encoding::{DescriptorEncoder, EncodeLabelSet, EncodeMetric},
  metrics::gauge::ConstGauge,
};
use std::{error::Error, sync::Arc};
use tokio::{runtime::Handle, task};

use crate::extension::{DockerExt, DockerStatsExt};

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

  // TODO: move logic into smaller functions -> do one thing well
  async fn metrics<'a>(&self) -> Result<Vec<DockerMetric<'a>>, Box<dyn Error>> {
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
      .await
      .unwrap_or_default()
      .into_iter()
      .flat_map(|(state, stats)| {
        Vec::from([
          DockerMetric::new(
            "name1",
            "help1",
            ConstGauge::new(
              stats
                .as_ref()
                .and_then(|s| s.cpu_utilization())
                .unwrap_or_default(),
            ),
          ),
          DockerMetric::new(
            "name2",
            "help2",
            ConstGauge::new(
              stats
                .as_ref()
                .and_then(|s| s.cpu_utilization())
                .unwrap_or_default(),
            ),
          ),
        ])
      })
      .collect();

    Ok(metrics)
  }
}

// TODO: do not unwrap
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

          let metrics = self.metrics().await.unwrap();

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

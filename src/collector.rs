use bollard::Docker;
use prometheus_client::{
  collector::Collector,
  encoding::{DescriptorEncoder, EncodeLabelSet, EncodeMetric, MetricEncoder},
  metrics::{counter::ConstCounter, MetricType, TypedMetric},
};
use std::error::Error;
use tokio::{runtime::Handle, task};

#[derive(Debug)]
pub struct DockerCollector {
  docker: Docker,
}

impl DockerCollector {
  pub fn new(docker: Docker) -> Self {
    return Self { docker };
  }

  async fn get_state_metrics<'a>(&self) -> Result<Vec<DockerMetric<'a>>, Box<dyn Error>> {
    let metric = ConstCounter::new(42);
    let metric2 = ConstCounter::new(84);

    Ok(Vec::from([
      DockerMetric {
        name: "metric",
        help: "help",
        metric: Box::new(metric),
      },
      DockerMetric {
        name: "metric2",
        help: "help2",
        metric: Box::new(metric2),
      },
    ]))
  }

  async fn get_cpu_metrics(&self) {}

  async fn get_memory_metrics(&self) {}

  async fn get_block_metrics(&self) {}

  async fn get_network_metrics(&self) {}
}

// TODO: do not unwrap
impl Collector for DockerCollector {
  fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
    // Blocking is required as the method is not available as async.
    task::block_in_place(move || {
      // Reentering the async context.
      Handle::current()
        .block_on(async move {
          let labels = DockerMetricLabels {
            container_name: "container_name".to_string(),
          };

          let metrics = self.get_state_metrics().await?;

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

#[derive(Clone, Debug, Default, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct DockerMetricLabels {
  pub container_name: String,
}

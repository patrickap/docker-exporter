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

  async fn collect(&self) -> Result<Vec<DockerMetric>, Box<dyn Error>> {
    // TODO: implement collecting of metrics
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    let state_metrics = self.get_state_metrics();
    Ok(state_metrics)
  }

  fn get_state_metrics(&self) -> Vec<DockerMetric> {
    let labels = DockerMetricLabels {
      container_name: "asd".to_string(),
    };

    let labels2 = DockerMetricLabels {
      container_name: "asd".to_string(),
    };

    let metric = ConstCounter::new(42);
    let metric2 = ConstCounter::new(84);

    vec![
      DockerMetric {
        name: "metric",
        help: "help",
        metric: Box::new(metric),
        labels: labels,
      },
      DockerMetric {
        name: "metric2",
        help: "help2",
        metric: Box::new(metric2),
        labels: labels2,
      },
    ]
  }
  fn get_cpu_metrics(&self) {}
  fn get_memory_metrics(&self) {}
  fn get_block_metrics(&self) {}
  fn get_network_metrics(&self) {}
}

// TODO: do not unwrap
impl Collector for DockerCollector {
  fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
    // Blocking is required as the method is not available as async.
    task::block_in_place(move || {
      // Reentering the async context.
      Handle::current()
        .block_on(async move {
          let metrics = self.collect().await?;

          metrics.iter().for_each(|metric| {
            let mut metric_encoder = encoder
              .encode_descriptor(metric.name, metric.help, None, metric.metric.metric_type())
              .unwrap();
            let metric_encoder = metric_encoder.encode_family(&metric.labels).unwrap();

            metric.metric.encode(metric_encoder).unwrap();
          });

          Ok::<(), Box<dyn std::error::Error>>(())
        })
        .unwrap()
    });

    Ok(())
  }
}

pub struct DockerMetric {
  name: &'static str,
  help: &'static str,
  metric: Box<dyn EncodeMetric + 'static>,
  labels: DockerMetricLabels,
}

#[derive(Clone, Debug, Default, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct DockerMetricLabels {
  pub container_name: String,
}

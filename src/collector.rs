use bollard::Docker;
use prometheus_client::{
  collector::Collector,
  encoding::{DescriptorEncoder, EncodeLabelSet, EncodeMetric},
  metrics::counter::ConstCounter,
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

  async fn collect(&self) -> Result<(), Box<dyn Error>> {
    // TODO: implement collecting of metrics
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    Ok(())
  }

  fn get_state_metrics(&self) {}
  fn get_cpu_metrics(&self) {}
  fn get_memory_metrics(&self) {}
  fn get_block_metrics(&self) {}
  fn get_network_metrics(&self) {}
}

// TODO: do not unwrap
impl Collector for DockerCollector {
  fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
    task::block_in_place(move || {
      Handle::current()
        .block_on(async move {
          let metrics = self.collect().await?;

          // TODO: for all metrics do the following
          let counter = ConstCounter::new(42);
          let labels = DockerMetricLabels {
            container_name: "asd".to_string(),
          };
          let mut metric_encoder =
            encoder.encode_descriptor("my_counter", "some help", None, counter.metric_type())?;
          let result = metric_encoder.encode_family(&labels)?;
          counter.encode(result)?;

          Ok::<(), Box<dyn std::error::Error>>(())
        })
        .unwrap()
    });

    Ok(())
  }
}

#[derive(Clone, Debug, Default, EncodeLabelSet, Eq, Hash, PartialEq)]
pub struct DockerMetricLabels {
  pub container_name: String,
}

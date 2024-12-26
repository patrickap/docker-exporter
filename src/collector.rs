use bollard::Docker;
use prometheus_client::{
  collector::Collector,
  encoding::{DescriptorEncoder, EncodeMetric},
  metrics::counter::ConstCounter,
};
use tokio::{runtime::Handle, task};

#[derive(Debug)]
pub struct DockerCollector {
  docker: Docker,
}

impl DockerCollector {
  pub fn new(docker: Docker) -> Self {
    return Self { docker };
  }

  async fn collect(&self) -> Result<(), Box<dyn std::error::Error>> {
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    Ok(())
  }
}

impl Collector for DockerCollector {
  fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
    // TODO: how to get data back from thread?
    let metrics = task::block_in_place(move || {
      Handle::current()
        .block_on(async move { self.collect().await })
        // TODO: do not unwrap
        .unwrap()
    });

    let counter = ConstCounter::new(42);
    let metric_encoder =
      encoder.encode_descriptor("my_counter", "some help", None, counter.metric_type())?;
    counter.encode(metric_encoder)?;

    Ok(())
  }
}

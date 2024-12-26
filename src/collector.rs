use bollard::Docker;
use prometheus_client::{
  collector::Collector,
  encoding::{DescriptorEncoder, EncodeMetric},
  metrics::counter::ConstCounter,
};

#[derive(Debug)]
pub struct DockerCollector {
  docker: Docker,
}

impl DockerCollector {
  pub fn new(docker: Docker) -> Self {
    return Self { docker };
  }
}

impl Collector for DockerCollector {
  fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
    let counter = ConstCounter::new(42);

    let metric_encoder =
      encoder.encode_descriptor("my_counter", "some help", None, counter.metric_type())?;
    counter.encode(metric_encoder)?;

    Ok(())
  }
}

use bollard::{container, Docker};
use futures::StreamExt;
use prometheus_client::{
  collector::Collector,
  encoding::{self, EncodeLabelSet, EncodeMetric},
  metrics::{self, counter, family::Family, gauge::Gauge},
  registry::Registry,
};
use std::sync::{Arc, Mutex};
use tokio;

#[derive(Debug)]
pub struct DockerCollector {
  client: Docker,
}

impl DockerCollector {
  pub fn new(client: Docker) -> Self {
    DockerCollector { client }
  }

  pub async fn collect_metrics(&self) {
    let containers = self
      .client
      .list_containers(Some(container::ListContainersOptions::<&str> {
        all: true,
        ..Default::default()
      }))
      .await
      .unwrap_or_default();

    for container in containers {
      let running = container.state.eq(&Some(String::from("running")));

      let name = Arc::new(
        container
          .names
          .and_then(|names| Some(names.join(";")))
          .map(|names| names[1..].to_string())
          .unwrap_or_default(),
      );

      // TODO: do not unwrap
      let stats = Arc::new(
        self
          .client
          .stats(&container.id.unwrap_or_default(), Default::default())
          .take(1)
          .next()
          .await
          .unwrap()
          .unwrap(),
      );

      tokio::spawn(DockerCollector::new_state_metric(
        running,
        Arc::clone(&name),
        Arc::clone(&stats),
      ));

      if running {
        tokio::spawn(DockerCollector::new_cpu_metric(
          Arc::clone(&name),
          Arc::clone(&stats),
        ));
        tokio::spawn(DockerCollector::new_memory_metric(
          Arc::clone(&name),
          Arc::clone(&stats),
        ));
        tokio::spawn(DockerCollector::new_io_metric(
          Arc::clone(&name),
          Arc::clone(&stats),
        ));
        tokio::spawn(DockerCollector::new_network_metric(
          Arc::clone(&name),
          Arc::clone(&stats),
        ));
      }
    }
  }

  async fn new_state_metric(running: bool, name: Arc<String>, stats: Arc<container::Stats>) {
    println!("1. collecting state metrics");
  }

  async fn new_cpu_metric(name: Arc<String>, stats: Arc<container::Stats>) {
    println!("2. collecting cpu metrics");
  }

  async fn new_memory_metric(name: Arc<String>, stats: Arc<container::Stats>) {
    println!("3. collecting memory metrics");
  }

  async fn new_io_metric(name: Arc<String>, stats: Arc<container::Stats>) {
    println!("4. collecting io metrics");
  }

  async fn new_network_metric(name: Arc<String>, stats: Arc<container::Stats>) {
    println!("5. collecting network metrics");
  }
}

impl Collector for DockerCollector {
  fn encode(&self, mut encoder: encoding::DescriptorEncoder) -> Result<(), std::fmt::Error> {
    let counter = counter::ConstCounter::new(42);
    let metric_encoder =
      encoder.encode_descriptor("my_counter", "some help", None, counter.metric_type())?;
    counter.encode(metric_encoder)?;
    Ok(())
  }
}

// TODO: tests

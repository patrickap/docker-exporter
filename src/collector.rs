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

  pub async fn collect_metrics(self: Arc<Self>) {
    let options = Some(container::ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    });

    let containers = self
      .client
      .list_containers(options)
      .await
      .unwrap_or_default();

    for container in containers {
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

      let running = container.state.eq(&Some(String::from("running")));

      tokio::spawn(Arc::clone(&self).collect_state_metrics(
        Arc::clone(&name),
        Arc::clone(&stats),
        running,
      ));

      if running {
        tokio::spawn(Arc::clone(&self).collect_cpu_metrics(Arc::clone(&name), Arc::clone(&stats)));
        tokio::spawn(
          Arc::clone(&self).collect_memory_metrics(Arc::clone(&name), Arc::clone(&stats)),
        );
        tokio::spawn(Arc::clone(&self).collect_io_metrics(Arc::clone(&name), Arc::clone(&stats)));
        tokio::spawn(
          Arc::clone(&self).collect_network_metrics(Arc::clone(&name), Arc::clone(&stats)),
        );
      }
    }
  }

  async fn collect_state_metrics(
    self: Arc<Self>,
    name: Arc<String>,
    stats: Arc<container::Stats>,
    running: bool,
  ) {
    // TODO: move struct to top of file
    #[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
    struct Labels {
      container_name: String,
    }

    let state_metric = Family::<Labels, Gauge>::default();
    state_metric
      .get_or_create(&Labels {
        container_name: name.to_string(),
      })
      .set(running as i64);

    println!("1. collecting state metrics");
  }

  async fn collect_cpu_metrics(self: Arc<Self>, name: Arc<String>, stats: Arc<container::Stats>) {
    println!("2. collecting cpu metrics");
  }

  async fn collect_memory_metrics(
    self: Arc<Self>,
    name: Arc<String>,
    stats: Arc<container::Stats>,
  ) {
    println!("3. collecting memory metrics");
  }

  async fn collect_io_metrics(self: Arc<Self>, name: Arc<String>, stats: Arc<container::Stats>) {
    println!("4. collecting io metrics");
  }

  async fn collect_network_metrics(
    self: Arc<Self>,
    name: Arc<String>,
    stats: Arc<container::Stats>,
  ) {
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

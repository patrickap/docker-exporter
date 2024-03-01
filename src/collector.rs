use bollard::{container, Docker};
use futures::{executor, StreamExt};
use prometheus_client::{
  collector::Collector,
  encoding::{self, EncodeLabelSet},
  metrics::{family::Family, gauge::Gauge},
  registry::Metric,
};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct DockerMetric {
  name: String,
  help: String,
  metric: Box<dyn Metric>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct DockerMetricLabels {
  container_name: String,
}
#[derive(Debug)]
pub struct DockerCollector {
  client: Docker,
}

impl DockerCollector {
  pub fn new(client: Docker) -> Self {
    DockerCollector { client }
  }

  pub async fn collect_metrics(&self) -> mpsc::Receiver<DockerMetric> {
    let containers = self
      .client
      .list_containers(Some(container::ListContainersOptions::<&str> {
        all: true,
        ..Default::default()
      }))
      .await
      .unwrap_or_default();

    let (tx, rx) = mpsc::channel::<DockerMetric>(32);
    let tx = Arc::new(tx);

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

      let tx = Arc::clone(&tx);

      tokio::spawn(async move {
        let metric =
          DockerCollector::new_state_metric(running, Arc::clone(&name), Arc::clone(&stats));
        // TODO: do not unwrap
        tx.send(metric).await.unwrap();
      });

      // if running {
      //   tokio::spawn(DockerCollector::new_cpu_metric(
      //     Arc::clone(&name),
      //     Arc::clone(&stats),
      //   ));
      //   tokio::spawn(DockerCollector::new_memory_metric(
      //     Arc::clone(&name),
      //     Arc::clone(&stats),
      //   ));
      //   tokio::spawn(DockerCollector::new_io_metric(
      //     Arc::clone(&name),
      //     Arc::clone(&stats),
      //   ));
      //   tokio::spawn(DockerCollector::new_network_metric(
      //     Arc::clone(&name),
      //     Arc::clone(&stats),
      //   ));
      // }
    }

    rx
  }

  fn new_state_metric(
    running: bool,
    name: Arc<String>,
    stats: Arc<container::Stats>,
  ) -> DockerMetric {
    let metric = Family::<DockerMetricLabels, Gauge>::default();
    metric
      .get_or_create(&DockerMetricLabels {
        // TODO: is to_string here ok?
        container_name: name.to_string(),
      })
      .set(running as i64);

    DockerMetric {
      name: String::from("container_running"),
      help: String::from("container running (1 = running, 0 = other)"),
      metric: Box::new(metric),
    }
  }

  fn new_cpu_metric(name: Arc<String>, stats: Arc<container::Stats>) {
    println!("2. collecting cpu metrics");
  }

  fn new_memory_metric(name: Arc<String>, stats: Arc<container::Stats>) {
    println!("3. collecting memory metrics");
  }

  fn new_io_metric(name: Arc<String>, stats: Arc<container::Stats>) {
    println!("4. collecting io metrics");
  }

  fn new_network_metric(name: Arc<String>, stats: Arc<container::Stats>) {
    println!("5. collecting network metrics");
  }
}

impl Collector for DockerCollector {
  fn encode(&self, mut encoder: encoding::DescriptorEncoder) -> Result<(), std::fmt::Error> {
    tokio::task::block_in_place(|| {
      let mut rx = executor::block_on(self.collect_metrics());
      while let Some(metric) = rx.blocking_recv() {
        // TODO: do not unwrap
        let metric_encoder = encoder
          .encode_descriptor(
            &metric.name,
            &metric.help,
            None,
            metric.metric.metric_type(),
          )
          .unwrap();

        // TODO: do not unwrap
        metric.metric.encode(metric_encoder).unwrap();
      }
    });

    Ok(())
  }
}

// TODO: tests

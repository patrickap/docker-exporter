use bollard::{container, Docker};
use futures::StreamExt;
use prometheus_client::{
  collector, encoding,
  metrics::{family, gauge},
  registry,
};
use std::sync::Arc;
use tokio::sync::mpsc;

pub trait Collector<S> {
  type Metric;

  fn new() -> Self;
  fn collect(
    &self,
    source: S,
    // TODO: makes this param and Arc<_> type sense when there can only be one receiver?
    channel: (
      Arc<mpsc::Sender<Self::Metric>>,
      Arc<mpsc::Receiver<Self::Metric>>,
    ),
  ) -> ();
}

pub struct DockerMetric {
  name: String,
  help: String,
  metric: Box<dyn registry::Metric>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, encoding::EncodeLabelSet)]
pub struct DockerMetricLabels {
  container_name: String,
}

#[derive(Debug)]
pub struct DockerCollector {}

impl Collector<Docker> for DockerCollector {
  type Metric = DockerMetric;

  fn new() -> Self {
    Self {}
  }

  fn collect(
    &self,
    docker: Docker,
    channel: (
      Arc<mpsc::Sender<Self::Metric>>,
      Arc<mpsc::Receiver<Self::Metric>>,
    ),
  ) {
    tokio::spawn(async move {
      let containers = docker
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

        let stats = Arc::new(
          docker
            .stats(&container.id.unwrap_or_default(), Default::default())
            .take(1)
            .next()
            .await
            .unwrap()
            .unwrap(),
        );

        let (ref tx, _) = channel;

        let tx = Arc::clone(&tx);
        tokio::spawn(async move {
          let metric =
            DockerCollector::new_state_metric(running, Arc::clone(&name), Arc::clone(&stats));
          // TODO: do not unwrap
          tx.send(metric).await.unwrap();
        });

        if running {
          // TODO: collect other metrics
        }
      }
    });
  }
}

impl DockerCollector {
  pub fn new_state_metric(
    running: bool,
    name: Arc<String>,
    stats: Arc<container::Stats>,
  ) -> DockerMetric {
    let metric = family::Family::<DockerMetricLabels, gauge::Gauge>::default();
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
  pub fn new_cpu_metric(name: Arc<String>, stats: Arc<container::Stats>) {}
  pub fn new_memory_metric(name: Arc<String>, stats: Arc<container::Stats>) {}
  pub fn new_io_metric(name: Arc<String>, stats: Arc<container::Stats>) {}
  pub fn new_network_metric(name: Arc<String>, stats: Arc<container::Stats>) {}
}

impl collector::Collector for DockerCollector {
  fn encode(&self, mut encoder: encoding::DescriptorEncoder) -> Result<(), std::fmt::Error> {
    // TODO: comment why this is all needed, there is no async implementation so we need to execute this blocking
    // TODO: do not unwrap
    let docker = Docker::connect_with_socket_defaults().unwrap();
    let (tx, rx) = mpsc::channel::<DockerMetric>(32);
    self.collect(docker, (Arc::new(tx), Arc::new(rx)));

    // spawn local as DescriptorEncoder is not thread safe
    tokio::task::spawn_local(async {
      while let Some(metric) = rx.recv().await {
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

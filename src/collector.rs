use bollard::{container, Docker};
use futures::StreamExt;
use prometheus_client::{
  collector, encoding,
  metrics::{family, gauge},
  registry,
};
use std::{
  cell::RefCell,
  sync::{Arc, Mutex, RwLock},
};
use tokio::sync::mpsc;

pub trait Collector<S> {
  type Metric;

  fn new() -> Self;
  fn collect(
    &self,
    source: &S,
    channel: &(mpsc::Sender<Self::Metric>, mpsc::Receiver<Self::Metric>),
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
    docker: &Docker,
    channel: &(mpsc::Sender<Self::Metric>, mpsc::Receiver<Self::Metric>),
  ) {
    tokio::spawn(async {
      let containers = docker
        .list_containers(Some(container::ListContainersOptions::<&str> {
          all: true,
          ..Default::default()
        }))
        .await
        .unwrap_or_default();

      for container in containers {
        tokio::spawn(async {
          let running = container.state.eq(&Some(String::from("running")));

          let name = Arc::new(
            container
              .names
              .and_then(|names| Some(names.join(";")))
              .map(|names| names[1..].to_string())
              .unwrap_or_default(),
          );

          // TODO: do not unwrap, should not await here to not block the loop
          let stats = Arc::new(
            docker
              .stats(&container.id.unwrap_or_default(), Default::default())
              .take(1)
              .next()
              .await
              .unwrap()
              .unwrap(),
          );

          let (tx, _) = channel;

          tokio::spawn(async {
            let metric =
              DockerCollector::new_state_metric(running, Arc::clone(&name), Arc::clone(&stats));
            tx.send(metric)
          });
        });
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
    let channel = mpsc::channel::<DockerMetric>(32);
    self.collect(&docker, &channel);

    let encoder = Arc::new(RefCell::new(encoder));

    tokio::task::spawn(async {
      let (_, rx) = channel;
      while let Some(metric) = rx.recv().await {
        let e = &encoder.clone();
      }
    });

    // tokio::task::block_in_place(|| {
    //   while let Some(metric) = rx.blocking_recv() {
    //     // TODO: do not unwrap
    //     let metric_encoder = encoder
    //       .encode_descriptor(
    //         &metric.name,
    //         &metric.help,
    //         None,
    //         metric.metric.metric_type(),
    //       )
    //       .unwrap();

    //     // TODO: do not unwrap
    //     metric.metric.encode(metric_encoder).unwrap();
    //   }
    // });

    Ok(())
  }
}

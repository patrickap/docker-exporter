use bollard::{container, Docker};
use futures::{future, StreamExt};
use prometheus_client::{
  collector, encoding,
  metrics::{family, gauge},
  registry,
};
use std::sync::Arc;
use tokio::{runtime::Handle, task};

pub trait Collector<S> {
  type Metric;

  fn new() -> Self;
  async fn collect(&self, source: Arc<S>) -> Vec<Result<Self::Metric, task::JoinError>>;
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

  async fn collect(&self, docker: Arc<Docker>) -> Vec<Result<Self::Metric, task::JoinError>> {
    let containers = docker
      .list_containers(Some(container::ListContainersOptions::<&str> {
        all: true,
        ..Default::default()
      }))
      .await
      .unwrap_or_default();

    let mut tasks = Vec::new();
    for container in containers {
      let docker = Arc::clone(&docker);

      tasks.push(task::spawn(async move {
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
          docker
            .stats(&container.id.unwrap_or_default(), Default::default())
            .take(1)
            .next()
            .await
            .unwrap()
            .unwrap(),
        );

        let metrics = Vec::from([task::spawn(async move {
          DockerCollector::new_state_metric(running, Arc::clone(&name), Arc::clone(&stats))
        })]);

        // TODO: add missing metrics

        future::join_all(metrics).await
      }));
    }
    let result = future::join_all(tasks)
      .await
      .into_iter()
      .map(|r| r.unwrap_or_default())
      .flatten()
      .collect();

    result
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
    task::block_in_place(|| {
      Handle::current().block_on(async {
        // TODO: do not unwrap
        let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
        let metrics = self.collect(docker).await;

        for metric in metrics {
          if let Ok(metric) = metric {
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
        }
      });
    });

    Ok(())
  }
}

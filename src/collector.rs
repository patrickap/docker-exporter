use bollard::{container, Docker};
use futures::{future, FutureExt, StreamExt};
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
  unit: Option<registry::Unit>,
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

    let tasks: Vec<_> = containers
      .into_iter()
      .map(|container| {
        let docker = Arc::clone(&docker);

        task::spawn(async move {
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

          let metrics = [
            Self::new_state_metric,
            Self::new_cpu_metric,
            Self::new_memory_metric,
            Self::new_io_metric,
            Self::new_network_metric,
          ]
          .map(|metric| {
            let name = Arc::clone(&name);
            let stats = Arc::clone(&stats);

            task::spawn(async move { metric(name, stats, running) })
          });

          future::join_all(metrics).await
        })
      })
      .collect();

    future::join_all(tasks)
      .await
      .into_iter()
      .map(|metrics| metrics.unwrap_or_default())
      .flatten()
      .collect()
  }
}

impl DockerCollector {
  pub fn new_state_metric(
    name: Arc<String>,
    stats: Arc<container::Stats>,
    running: bool,
  ) -> DockerMetric {
    let metric = family::Family::<DockerMetricLabels, gauge::Gauge>::default();
    metric
      .get_or_create(&DockerMetricLabels {
        container_name: name.to_string(),
      })
      .set(running as i64);

    DockerMetric {
      name: String::from("container_running"),
      help: String::from("container running (1 = running, 0 = other)"),
      unit: None,
      metric: Box::new(metric),
    }
  }

  pub fn new_cpu_metric(
    name: Arc<String>,
    stats: Arc<container::Stats>,
    running: bool,
  ) -> DockerMetric {
    DockerMetric {
      name: String::from("todo"),
      help: String::from("todo"),
      unit: None,
      metric: Box::new(family::Family::<DockerMetricLabels, gauge::Gauge>::default()),
    }
  }

  pub fn new_memory_metric(
    name: Arc<String>,
    stats: Arc<container::Stats>,
    running: bool,
  ) -> DockerMetric {
    DockerMetric {
      name: String::from("todo"),
      help: String::from("todo"),
      unit: None,
      metric: Box::new(family::Family::<DockerMetricLabels, gauge::Gauge>::default()),
    }
  }

  pub fn new_io_metric(
    name: Arc<String>,
    stats: Arc<container::Stats>,
    running: bool,
  ) -> DockerMetric {
    DockerMetric {
      name: String::from("todo"),
      help: String::from("todo"),
      unit: None,
      metric: Box::new(family::Family::<DockerMetricLabels, gauge::Gauge>::default()),
    }
  }

  pub fn new_network_metric(
    name: Arc<String>,
    stats: Arc<container::Stats>,
    running: bool,
  ) -> DockerMetric {
    DockerMetric {
      name: String::from("todo"),
      help: String::from("todo"),
      unit: None,
      metric: Box::new(family::Family::<DockerMetricLabels, gauge::Gauge>::default()),
    }
  }
}

impl collector::Collector for DockerCollector {
  fn encode(&self, mut encoder: encoding::DescriptorEncoder) -> Result<(), std::fmt::Error> {
    task::block_in_place(|| {
      Handle::current().block_on(async {
        let docker = match Docker::connect_with_socket_defaults() {
          Ok(docker) => Arc::new(docker),
          Err(err) => {
            eprintln!("failed to connect to docker daemon: {:?}", err);
            return;
          }
        };

        let metrics = self.collect(docker).await;

        for metric in metrics {
          if let Ok(DockerMetric {
            name,
            help,
            unit,
            metric,
          }) = metric
          {
            encoder
              .encode_descriptor(&name, &help, unit.as_ref(), metric.metric_type())
              .and_then(|encoder| metric.encode(encoder))
              .map_err(|err| {
                eprintln!("failed to encode metrics: {:?}", err);
                err
              })
              .unwrap_or_default()
          }
        }
      });
    });

    Ok(())
  }
}

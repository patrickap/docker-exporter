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

        let mut tasks = Vec::new();

        tasks.push(task::spawn(async move {
          Self::new_state_metric(Arc::clone(&name), running)
        }));

        // TODO: add missing metrics
        // if running {
        //   tasks.push(task::spawn(async move {
        //     Self::new_cpu_metric(Arc::clone(&name), Arc::clone(&stats))
        //   }));

        //   tasks.push(task::spawn(async move {
        //     Self::new_memory_metric(Arc::clone(&name), Arc::clone(&stats))
        //   }));

        //   tasks.push(task::spawn(async move {
        //     Self::new_io_metric(Arc::clone(&name), Arc::clone(&stats))
        //   }));

        //   tasks.push(task::spawn(async move {
        //     Self::new_network_metric(Arc::clone(&name), Arc::clone(&stats))
        //   }));
        // }

        future::join_all(tasks).await
      }));
    }

    future::join_all(tasks)
      .await
      .into_iter()
      .map(|metrics| metrics.unwrap_or_default())
      .flatten()
      .collect()
  }
}

impl DockerCollector {
  pub fn new_state_metric(name: Arc<String>, running: bool) -> DockerMetric {
    let metric = family::Family::<DockerMetricLabels, gauge::Gauge>::default();
    metric
      .get_or_create(&DockerMetricLabels {
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
        let docker = match Docker::connect_with_socket_defaults() {
          Ok(docker) => Arc::new(docker),
          Err(err) => {
            eprintln!("failed to connect to docker daemon: {:?}", err);
            return;
          }
        };

        let metrics = self.collect(docker).await;

        for metric in metrics {
          if let Ok(DockerMetric { name, help, metric }) = metric {
            encoder
              .encode_descriptor(&name, &help, None, metric.metric_type())
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

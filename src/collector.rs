use bollard::{container, Docker};
use futures::{future, FutureExt, StreamExt};
use prometheus_client::{
  collector, encoding,
  metrics::{family, gauge},
  registry,
};
use std::sync::{atomic::AtomicU64, Arc};
use tokio::{runtime::Handle, task};

pub trait Collector<S> {
  type Metric;

  fn new() -> Self;
  async fn collect(&self, source: Arc<S>) -> Vec<Self::Metric>;
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

  async fn collect(&self, docker: Arc<Docker>) -> Vec<Self::Metric> {
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
              .and_then(|mut name| Some(name.drain(1..).collect())),
          );

          let stats = Arc::new(
            docker
              .stats(
                &container.id.unwrap_or_default(),
                Some(container::StatsOptions {
                  stream: false,
                  ..Default::default()
                }),
              )
              .take(1)
              .next()
              .map(|stats| match stats {
                Some(Ok(stats)) => Some(stats),
                _ => None,
              })
              .await,
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

          future::join_all(metrics)
            .await
            .into_iter()
            .flat_map(|metric| match metric {
              Ok(Some(metric)) => Some(metric),
              _ => None,
            })
            .collect::<Vec<_>>()
        })
      })
      .collect();

    future::join_all(tasks)
      .await
      .into_iter()
      .flat_map(|task| match task {
        Ok(task) => task,
        _ => Default::default(),
      })
      .collect()
  }
}

impl DockerCollector {
  pub fn new_state_metric(
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    running: bool,
  ) -> Option<DockerMetric> {
    match (name.as_ref(), stats.as_ref()) {
      (Some(name), _) => {
        let metric = family::Family::<DockerMetricLabels, gauge::Gauge>::default();

        metric
          .get_or_create(&DockerMetricLabels {
            container_name: name.to_string(),
          })
          .set(running as i64);

        Some(DockerMetric {
          name: String::from("container_running"),
          help: String::from("container running (1 = running, 0 = other)"),
          unit: None,
          metric: Box::new(metric),
        })
      }
      _ => None,
    }
  }

  pub fn new_cpu_metric(
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    running: bool,
  ) -> Option<DockerMetric> {
    match (name.as_ref(), stats.as_ref(), running) {
      (Some(name), Some(stats), true) => {
        let cpu_delta =
          stats.cpu_stats.cpu_usage.total_usage - stats.precpu_stats.cpu_usage.total_usage;

        let system_cpu_delta =
          stats.cpu_stats.system_cpu_usage? - stats.precpu_stats.system_cpu_usage?;

        let number_cpus = stats.cpu_stats.online_cpus.unwrap_or(1);

        let cpu_utilization =
          (cpu_delta as f64 / system_cpu_delta as f64) * number_cpus as f64 * 100.0;

        let metric = family::Family::<DockerMetricLabels, gauge::Gauge<f64, AtomicU64>>::default();

        metric
          .get_or_create(&DockerMetricLabels {
            container_name: name.to_string(),
          })
          .set(cpu_utilization);

        Some(DockerMetric {
          name: String::from("cpu_utilization_percent"),
          help: String::from("cpu utilization in percent"),
          unit: None,
          metric: Box::new(metric),
        })
      }

      _ => None,
    }
  }

  pub fn new_memory_metric(
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    running: bool,
  ) -> Option<DockerMetric> {
    Some(DockerMetric {
      name: String::from("todo"),
      help: String::from("todo"),
      unit: None,
      metric: Box::new(family::Family::<DockerMetricLabels, gauge::Gauge>::default()),
    })
  }

  pub fn new_io_metric(
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    running: bool,
  ) -> Option<DockerMetric> {
    Some(DockerMetric {
      name: String::from("todo"),
      help: String::from("todo"),
      unit: None,
      metric: Box::new(family::Family::<DockerMetricLabels, gauge::Gauge>::default()),
    })
  }

  pub fn new_network_metric(
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    running: bool,
  ) -> Option<DockerMetric> {
    Some(DockerMetric {
      name: String::from("todo"),
      help: String::from("todo"),
      unit: None,
      metric: Box::new(family::Family::<DockerMetricLabels, gauge::Gauge>::default()),
    })
  }
}

impl collector::Collector for DockerCollector {
  fn encode(&self, mut encoder: encoding::DescriptorEncoder) -> Result<(), std::fmt::Error> {
    // Prometheus does not provide an async encode function
    // This requires bridging between async and sync, resulting in a blocking operation
    // To prevent blocking the async executor, block_in_place is used
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
          let DockerMetric {
            name,
            help,
            unit,
            metric,
          } = metric;

          encoder
            .encode_descriptor(&name, &help, unit.as_ref(), metric.metric_type())
            .and_then(|encoder| metric.encode(encoder))
            .map_err(|err| {
              eprintln!("failed to encode metrics: {:?}", err);
              err
            })
            .unwrap_or_default()
        }
      });
    });

    Ok(())
  }
}

use bollard::{container, Docker};
use futures::{future, FutureExt, StreamExt};
use prometheus_client::{
  collector, encoding,
  metrics::{counter, family, gauge},
  registry,
};
use std::sync::{atomic::AtomicU64, Arc};
use tokio::{runtime::Handle, sync::mpsc, task};

pub trait Collector<S> {
  type Metric;

  fn new() -> Self;
  async fn collect(&self, source: Arc<S>, tx: Arc<mpsc::Sender<Self::Metric>>);
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

  async fn collect(&self, docker: Arc<Docker>, tx: Arc<mpsc::Sender<Self::Metric>>) {
    let containers = docker
      .list_containers(Some(container::ListContainersOptions::<&str> {
        all: true,
        ..Default::default()
      }))
      .await
      .unwrap_or_default();

    for container in containers {
      let docker = Arc::clone(&docker);
      let tx = Arc::clone(&tx);

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

        task::spawn(Self::new_state_metric(
          running,
          Arc::clone(&name),
          Arc::clone(&stats),
          Arc::clone(&tx),
        ));

        task::spawn(Self::new_cpu_metric(
          running,
          Arc::clone(&name),
          Arc::clone(&stats),
          Arc::clone(&tx),
        ));

        task::spawn(Self::new_memory_metric(
          running,
          Arc::clone(&name),
          Arc::clone(&stats),
          Arc::clone(&tx),
        ));

        task::spawn(Self::new_io_metric(
          running,
          Arc::clone(&name),
          Arc::clone(&stats),
          Arc::clone(&tx),
        ));

        task::spawn(Self::new_network_metric(
          running,
          Arc::clone(&name),
          Arc::clone(&stats),
          Arc::clone(&tx),
        ));
      });
    }
  }
}

impl DockerCollector {
  pub async fn new_state_metric(
    running: bool,
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    tx: Arc<mpsc::Sender<DockerMetric>>,
  ) {
    if let (_, Some(name), _) = (running, name.as_ref(), stats.as_ref()) {
      let metric = family::Family::<DockerMetricLabels, gauge::Gauge>::default();

      metric
        .get_or_create(&DockerMetricLabels {
          container_name: name.to_string(),
        })
        .set(running as i64);

      task::spawn(async move {
        tx.send(DockerMetric {
          name: String::from("container_running"),
          help: String::from("container running (1 = running, 0 = other)"),
          unit: None,
          metric: Box::new(metric),
        })
        .await
      });
    }
  }

  pub async fn new_cpu_metric(
    running: bool,
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    tx: Arc<mpsc::Sender<DockerMetric>>,
  ) {
    if let (true, Some(name), Some(stats)) = (running, name.as_ref(), stats.as_ref()) {
      let cpu_delta =
        stats.cpu_stats.cpu_usage.total_usage - stats.precpu_stats.cpu_usage.total_usage;

      let system_cpu_delta = match (
        stats.cpu_stats.system_cpu_usage,
        stats.precpu_stats.system_cpu_usage,
      ) {
        (Some(system_cpu_usage), Some(system_precpu_usage)) => {
          Some(system_cpu_usage - system_precpu_usage)
        }
        _ => None,
      };

      let number_cpus = stats.cpu_stats.online_cpus.or(Some(1));

      match (cpu_delta, system_cpu_delta, number_cpus) {
        (cpu_delta, Some(system_cpu_delta), Some(number_cpus)) => {
          let cpu_utilization =
            (cpu_delta as f64 / system_cpu_delta as f64) * number_cpus as f64 * 100.0;

          let metric =
            family::Family::<DockerMetricLabels, gauge::Gauge<f64, AtomicU64>>::default();

          metric
            .get_or_create(&DockerMetricLabels {
              container_name: name.to_string(),
            })
            .set(cpu_utilization);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("cpu_utilization_percent"),
              help: String::from("cpu utilization in percent"),
              unit: None,
              metric: Box::new(metric),
            })
            .await
          });
        }
        _ => {}
      };
    }
  }

  pub async fn new_memory_metric(
    running: bool,
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    tx: Arc<mpsc::Sender<DockerMetric>>,
  ) {
    if let (true, Some(name), Some(stats)) = (running, name.as_ref(), stats.as_ref()) {
      let memory_usage = match (stats.memory_stats.usage, stats.memory_stats.stats) {
        (Some(memory_usage), Some(container::MemoryStatsStats::V1(memory_stats))) => {
          Some(memory_usage - memory_stats.cache)
        }
        (Some(memory_usage), Some(container::MemoryStatsStats::V2(_))) => {
          // TODO: how to calculate the cache using v2 cgroups?
          Some(memory_usage - 0)
        }
        _ => None,
      };

      let memory_total = stats.memory_stats.limit;

      match (memory_usage, memory_total) {
        (Some(memory_usage), Some(memory_total)) => {
          let memory_utilization = (memory_usage as f64 / memory_total as f64) * 100.0;

          let metric =
            family::Family::<DockerMetricLabels, gauge::Gauge<f64, AtomicU64>>::default();

          metric
            .get_or_create(&DockerMetricLabels {
              container_name: name.to_string(),
            })
            .set(memory_utilization);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("memory_utilization_percent"),
              help: String::from("memory utilization in percent"),
              unit: None,
              metric: Box::new(metric),
            })
            .await
          });
        }
        (Some(memory_usage), _) => {
          let metric =
            family::Family::<DockerMetricLabels, counter::Counter<f64, AtomicU64>>::default();

          metric
            .get_or_create(&DockerMetricLabels {
              container_name: name.to_string(),
            })
            .inc_by(memory_usage as f64);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("memory_usage_bytes"),
              help: String::from("memory usage in bytes"),
              unit: None,
              metric: Box::new(metric),
            })
            .await
          });
        }
        (_, Some(memory_total)) => {
          let metric =
            family::Family::<DockerMetricLabels, counter::Counter<f64, AtomicU64>>::default();

          metric
            .get_or_create(&DockerMetricLabels {
              container_name: name.to_string(),
            })
            .inc_by(memory_total as f64);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("memory_total_bytes"),
              help: String::from("memory total in bytes"),
              unit: None,
              metric: Box::new(metric),
            })
            .await
          });
        }
        _ => {}
      };
    }
  }

  pub async fn new_io_metric(
    running: bool,
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    tx: Arc<mpsc::Sender<DockerMetric>>,
  ) {
    task::spawn(async move {
      tx.send(DockerMetric {
        name: String::from("todo"),
        help: String::from("todo"),
        unit: None,
        metric: Box::new(family::Family::<DockerMetricLabels, gauge::Gauge>::default()),
      })
      .await
    });
  }

  pub async fn new_network_metric(
    running: bool,
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    tx: Arc<mpsc::Sender<DockerMetric>>,
  ) {
    task::spawn(async move {
      tx.send(DockerMetric {
        name: String::from("todo"),
        help: String::from("todo"),
        unit: None,
        metric: Box::new(family::Family::<DockerMetricLabels, gauge::Gauge>::default()),
      })
      .await
    });
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

        let (tx, mut rx) = mpsc::channel::<DockerMetric>(32);
        self.collect(docker, Arc::new(tx)).await;

        while let Some(DockerMetric {
          name,
          help,
          unit,
          metric,
        }) = rx.recv().await
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
      });
    });

    Ok(())
  }
}

use bollard::{
  container::{
    self, InspectContainerOptions, ListContainersOptions, MemoryStatsStats, StatsOptions,
  },
  models::ContainerState,
  Docker,
};
use futures::{FutureExt, StreamExt, TryStreamExt};
use prometheus_client::{
  collector::Collector,
  encoding::{DescriptorEncoder, EncodeLabelSet},
  metrics::{
    counter::Counter,
    family::Family,
    gauge::{Atomic, Gauge},
  },
  registry::{Metric, Unit},
};
use std::{
  env,
  fmt::Error,
  sync::{atomic::AtomicU64, Arc},
};
use tokio::{
  runtime::Handle,
  sync::mpsc::{self, Sender},
  task,
};

use crate::config::constants::{
  DEFAULT_DOCKER_API_VERSION, DEFAULT_DOCKER_CONNECTION_TIMEOUT, DEFAULT_DOCKER_SOCKET_PATH,
  DOCKER_HOST_ENV,
};

pub trait DefaultCollector<S> {
  type Metric;

  fn new() -> Self;
  async fn collect(&self, source: Arc<S>, tx: Arc<Sender<Self::Metric>>);
}

#[derive(Debug)]
pub struct DockerCollector {}

pub struct DockerMetric {
  name: String,
  help: String,
  unit: Option<Unit>,
  metric: Box<dyn Metric>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct DockerMetricLabels {
  container_name: String,
}

impl DefaultCollector<Docker> for DockerCollector {
  type Metric = DockerMetric;

  fn new() -> Self {
    Self {}
  }

  async fn collect(&self, docker: Arc<Docker>, tx: Arc<Sender<Self::Metric>>) {
    let containers = docker
      .list_containers(Some(ListContainersOptions::<&str> {
        all: true,
        ..Default::default()
      }))
      .await
      .unwrap_or_default();

    for container in containers {
      let docker = Arc::clone(&docker);
      let tx = Arc::clone(&tx);

      task::spawn(async move {
        let id = Arc::new(container.id);

        let name = Arc::new(
          container
            .names
            .and_then(|names| Some(names.join(";")))
            .and_then(|mut name| Some(name.drain(1..).collect())),
        );

        let state = {
          let docker = Arc::clone(&docker);
          let id = Arc::clone(&id);

          task::spawn(async move {
            docker
              .inspect_container(
                id.as_deref().unwrap_or_default(),
                Some(InspectContainerOptions {
                  ..Default::default()
                }),
              )
              .await
          })
          .map(|inspect| match inspect {
            Ok(Ok(inspect)) => Arc::new(inspect.state),
            _ => Arc::new(None),
          })
        };

        let stats = {
          let docker = Arc::clone(&docker);
          let id = Arc::clone(&id);

          task::spawn(async move {
            docker
              .stats(
                id.as_deref().unwrap_or_default(),
                Some(StatsOptions {
                  stream: false,
                  ..Default::default()
                }),
              )
              .take(1)
              .try_next()
              .await
          })
          .map(|stats| match stats {
            Ok(Ok(stats)) => Arc::new(stats),
            _ => Arc::new(None),
          })
        };

        let (state, stats) = tokio::join!(state, stats);

        task::spawn(Self::new_state_metric(
          Arc::clone(&name),
          Arc::clone(&state),
          Arc::clone(&tx),
        ));

        if let Some(state) = &*state {
          if let Some(true) = state.running {
            task::spawn(Self::new_cpu_metric(
              Arc::clone(&name),
              Arc::clone(&stats),
              Arc::clone(&tx),
            ));

            task::spawn(Self::new_memory_metric(
              Arc::clone(&name),
              Arc::clone(&stats),
              Arc::clone(&tx),
            ));

            task::spawn(Self::new_block_io_metric(
              Arc::clone(&name),
              Arc::clone(&stats),
              Arc::clone(&tx),
            ));

            task::spawn(Self::new_network_metric(
              Arc::clone(&name),
              Arc::clone(&stats),
              Arc::clone(&tx),
            ));
          }
        }
      });
    }
  }
}

impl DockerCollector {
  pub async fn new_state_metric(
    name: Arc<Option<String>>,
    state: Arc<Option<ContainerState>>,
    tx: Arc<Sender<DockerMetric>>,
  ) {
    match (&*name, &*state) {
      (Some(name), Some(state)) => {
        let gauge = Family::<DockerMetricLabels, Gauge>::default();

        gauge
          .get_or_create(&DockerMetricLabels {
            container_name: String::from(name),
          })
          .set(state.running.unwrap_or_default() as i64);

        task::spawn(async move {
          tx.send(DockerMetric {
            name: String::from("container_running_boolean"),
            help: String::from("container running as boolean (1 = true, 0 = false)"),
            unit: None,
            metric: Box::new(gauge),
          })
          .await
        });
      }
      _ => (),
    }
  }

  pub async fn new_cpu_metric(
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    tx: Arc<Sender<DockerMetric>>,
  ) {
    match (&*name, &*stats) {
      (Some(name), Some(stats)) => {
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

        if let (Some(system_cpu_delta), Some(number_cpus)) = (system_cpu_delta, number_cpus) {
          let cpu_utilization =
            (cpu_delta as f64 / system_cpu_delta as f64) * number_cpus as f64 * 100.0;

          let gauge = Family::<DockerMetricLabels, Gauge<f64, AtomicU64>>::default();

          gauge
            .get_or_create(&DockerMetricLabels {
              container_name: String::from(name),
            })
            .set(cpu_utilization);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("cpu_utilization_percent"),
              help: String::from("cpu utilization in percent"),
              unit: None,
              metric: Box::new(gauge),
            })
            .await
          });
        }
      }
      _ => (),
    }
  }

  pub async fn new_memory_metric(
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    tx: Arc<Sender<DockerMetric>>,
  ) {
    match (&*name, &*stats) {
      (Some(name), Some(stats)) => {
        let memory_usage = match (stats.memory_stats.usage, stats.memory_stats.stats) {
          (Some(memory_usage), Some(MemoryStatsStats::V1(memory_stats))) => {
            Some(memory_usage - memory_stats.cache)
          }
          (Some(memory_usage), Some(MemoryStatsStats::V2(_))) => {
            // In cgroup v2, Docker doesn't provide a cache property
            // Unfortunately, there's no simple way to differentiate cache from memory usage
            Some(memory_usage - 0)
          }
          _ => None,
        };

        let memory_total = stats.memory_stats.limit;

        if let Some(memory_usage) = memory_usage {
          let tx = Arc::clone(&tx);

          let gauge = Family::<DockerMetricLabels, Gauge<f64, AtomicU64>>::default();

          gauge
            .get_or_create(&DockerMetricLabels {
              container_name: String::from(name),
            })
            .set(memory_usage as f64);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("memory_usage_bytes"),
              help: String::from("memory usage in bytes"),
              unit: None,
              metric: Box::new(gauge),
            })
            .await
          });
        }

        if let Some(memory_total) = memory_total {
          let tx = Arc::clone(&tx);

          let counter = Family::<DockerMetricLabels, Counter<f64, AtomicU64>>::default();

          counter
            .get_or_create(&DockerMetricLabels {
              container_name: String::from(name),
            })
            .inner()
            .set(memory_total as f64);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("memory_bytes"),
              help: String::from("memory total in bytes"),
              unit: None,
              metric: Box::new(counter),
            })
            .await
          });
        }

        if let (Some(memory_usage), Some(memory_total)) = (memory_usage, memory_total) {
          let tx = Arc::clone(&tx);

          let memory_utilization = (memory_usage as f64 / memory_total as f64) * 100.0;

          let gauge = Family::<DockerMetricLabels, Gauge<f64, AtomicU64>>::default();

          gauge
            .get_or_create(&DockerMetricLabels {
              container_name: String::from(name),
            })
            .set(memory_utilization);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("memory_utilization_percent"),
              help: String::from("memory utilization in percent"),
              unit: None,
              metric: Box::new(gauge),
            })
            .await
          });
        }
      }
      _ => (),
    }
  }

  pub async fn new_block_io_metric(
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    tx: Arc<Sender<DockerMetric>>,
  ) {
    match (&*name, &*stats) {
      (Some(name), Some(stats)) => {
        let (block_io_tx, block_io_rx) = match stats.blkio_stats.io_service_bytes_recursive.as_ref()
        {
          Some(io) => {
            let (tx, rx) = io.iter().fold((0, 0), |acc, io| match io.op.as_str() {
              "write" => (acc.0 + io.value, acc.1),
              "read" => (acc.0, acc.1 + io.value),
              _ => acc,
            });

            (Some(tx), Some(rx))
          }
          _ => (None, None),
        };

        if let Some(block_io_tx) = block_io_tx {
          let tx = Arc::clone(&tx);

          let counter = Family::<DockerMetricLabels, Counter<f64, AtomicU64>>::default();

          counter
            .get_or_create(&DockerMetricLabels {
              container_name: String::from(name),
            })
            .inner()
            .set(block_io_tx as f64);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("block_io_tx_bytes"),
              help: String::from("block io read total in bytes"),
              unit: None,
              metric: Box::new(counter),
            })
            .await
          });
        }

        if let Some(block_io_rx) = block_io_rx {
          let tx = Arc::clone(&tx);

          let counter = Family::<DockerMetricLabels, Counter<f64, AtomicU64>>::default();

          counter
            .get_or_create(&DockerMetricLabels {
              container_name: String::from(name),
            })
            .inner()
            .set(block_io_rx as f64);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("block_io_rx_bytes"),
              help: String::from("block io written total in bytes"),
              unit: None,
              metric: Box::new(counter),
            })
            .await
          });
        }
      }
      _ => (),
    }
  }

  pub async fn new_network_metric(
    name: Arc<Option<String>>,
    stats: Arc<Option<container::Stats>>,
    tx: Arc<Sender<DockerMetric>>,
  ) {
    match (&*name, &*stats) {
      (Some(name), Some(stats)) => {
        let (network_tx, network_rx) = match stats
          .networks
          .as_ref()
          .and_then(|networks| networks.get("eth0"))
        {
          Some(networks) => (Some(networks.tx_bytes), Some(networks.rx_bytes)),
          _ => (None, None),
        };

        if let Some(network_tx) = network_tx {
          let tx = Arc::clone(&tx);

          let counter = Family::<DockerMetricLabels, Counter<f64, AtomicU64>>::default();

          counter
            .get_or_create(&DockerMetricLabels {
              container_name: String::from(name),
            })
            .inner()
            .set(network_tx as f64);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("network_tx_bytes"),
              help: String::from("network sent total in bytes"),
              unit: None,
              metric: Box::new(counter),
            })
            .await
          });
        }

        if let Some(network_rx) = network_rx {
          let tx = Arc::clone(&tx);

          let counter = Family::<DockerMetricLabels, Counter<f64, AtomicU64>>::default();

          counter
            .get_or_create(&DockerMetricLabels {
              container_name: String::from(name),
            })
            .inner()
            .set(network_rx as f64);

          task::spawn(async move {
            tx.send(DockerMetric {
              name: String::from("network_rx_bytes"),
              help: String::from("network received total in bytes"),
              unit: None,
              metric: Box::new(counter),
            })
            .await
          });
        }
      }
      _ => (),
    }
  }
}

impl Collector for DockerCollector {
  fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), Error> {
    // Prometheus does not provide an async encode function which requires bridging between async and sync
    // Unfortenately, task::spawn is not feasible as the encoder cannot be sent between threads safely
    // Local spawning via task::spawn_local is also not suitable as the function parameters would escape the method
    // Nevertheless, to prevent blocking the async executor, block_in_place is utilized instead
    task::block_in_place(|| {
      Handle::current().block_on(async {
        let docker_connection = match env::var(DOCKER_HOST_ENV) {
          Ok(docker_host) => Docker::connect_with_http(
            &docker_host,
            DEFAULT_DOCKER_CONNECTION_TIMEOUT,
            DEFAULT_DOCKER_API_VERSION,
          ),
          _ => Docker::connect_with_socket(
            DEFAULT_DOCKER_SOCKET_PATH,
            DEFAULT_DOCKER_CONNECTION_TIMEOUT,
            DEFAULT_DOCKER_API_VERSION,
          ),
        };

        let docker = match docker_connection {
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

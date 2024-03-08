use bollard::{
  container::{
    InspectContainerOptions, ListContainersOptions, MemoryStatsStats, Stats, StatsOptions,
  },
  models::ContainerState,
  Docker,
};
use futures::{FutureExt, StreamExt, TryStreamExt};
use prometheus_client::{
  collector::Collector,
  encoding::{DescriptorEncoder, EncodeLabelSet, EncodeMetric},
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
  ops::Deref,
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

pub trait StatsExt {
  fn cpu_delta(&self) -> Option<u64>;
  fn cpu_delta_system(&self) -> Option<u64>;
  fn cpu_count(&self) -> Option<u64>;
  fn cpu_utilization(&self) -> Option<f64>;
  fn memory_usage(&self) -> Option<u64>;
  fn memory_total(&self) -> Option<u64>;
  fn memory_utilization(&self) -> Option<f64>;
  fn block_io_total(&self) -> Option<(u64, u64)>;
  fn block_io_tx_total(&self) -> Option<u64>;
  fn block_io_rx_total(&self) -> Option<u64>;
  fn network_total(&self) -> Option<(u64, u64)>;
  fn network_tx_total(&self) -> Option<u64>;
  fn network_rx_total(&self) -> Option<u64>;
}

impl StatsExt for Stats {
  fn cpu_delta(&self) -> Option<u64> {
    Some(self.cpu_stats.cpu_usage.total_usage - self.precpu_stats.cpu_usage.total_usage)
  }

  fn cpu_delta_system(&self) -> Option<u64> {
    Some(self.cpu_stats.system_cpu_usage? - self.precpu_stats.system_cpu_usage?)
  }

  fn cpu_count(&self) -> Option<u64> {
    self.cpu_stats.online_cpus.or(Some(1))
  }

  fn cpu_utilization(&self) -> Option<f64> {
    Some(
      (self.cpu_delta()? as f64 / self.cpu_delta_system()? as f64)
        * self.cpu_count()? as f64
        * 100.0,
    )
  }

  fn memory_usage(&self) -> Option<u64> {
    let memory_cache = match self.memory_stats.stats? {
      MemoryStatsStats::V1(memory_stats) => memory_stats.cache,
      // In cgroup v2, Docker doesn't provide a cache property
      // Unfortunately, there's no simple way to differentiate cache from memory usage
      MemoryStatsStats::V2(_) => 0,
      _ => 0,
    };

    Some(self.memory_stats.usage? - memory_cache)
  }

  fn memory_total(&self) -> Option<u64> {
    self.memory_stats.limit
  }

  fn memory_utilization(&self) -> Option<f64> {
    Some((self.memory_usage()? as f64 / self.memory_total()? as f64) * 100.0)
  }

  fn block_io_total(&self) -> Option<(u64, u64)> {
    self
      .blkio_stats
      .io_service_bytes_recursive
      .as_ref()?
      .iter()
      .fold(Some((0, 0)), |acc, io| match io.op.as_str() {
        "write" => Some((acc?.0 + io.value, acc?.1)),
        "read" => Some((acc?.0, acc?.1 + io.value)),
        _ => acc,
      })
  }

  fn block_io_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.block_io_total()?;
    Some(tx)
  }

  fn block_io_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.block_io_total()?;
    Some(rx)
  }

  fn network_total(&self) -> Option<(u64, u64)> {
    let network = self.networks.as_ref()?.get("eth0")?;
    Some((network.tx_bytes, network.rx_bytes))
  }

  fn network_tx_total(&self) -> Option<u64> {
    let (tx, _) = self.network_total()?;
    Some(tx)
  }

  fn network_rx_total(&self) -> Option<u64> {
    let (_, rx) = self.network_total()?;
    Some(rx)
  }
}

pub struct ContainerMetric<M: Metric + ?Sized> {
  name: String,
  help: String,
  metric: Box<M>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, EncodeLabelSet)]
pub struct ContainerMetricLabels {
  container_name: String,
}

pub trait DefaultCollector<S> {
  type Metric;

  fn new() -> Self;
  async fn collect(&self, source: Arc<S>, tx: Arc<Sender<Self::Metric>>);
}

// TODO: enum for metric names?
// TODO: check again metrics calculation, names etc.

#[derive(Debug)]
pub struct DockerCollector {}

impl DefaultCollector<Docker> for DockerCollector {
  type Metric = ContainerMetric<dyn Metric>;

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
            .and_then(|mut name| Some(name.drain(1..).collect::<String>())),
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

        let labels = Arc::new(ContainerMetricLabels {
          container_name: String::from(Option::as_ref(&name).unwrap()),
        });

        // TODO: do not unwrap in tasks
        // TODO: DRY
        // TODO: tests

        {
          let state = Arc::clone(&state);
          let labels = Arc::clone(&labels);
          let tx = Arc::clone(&tx);

          task::spawn(async move {
            let metric = Family::<ContainerMetricLabels, Gauge>::default();
            metric
              .get_or_create(&labels)
              .set(Option::as_ref(&state).and_then(|s| s.running).unwrap() as i64);
            tx.send(ContainerMetric {
              name: String::from("container_running_boolean"),
              help: String::from("container running as boolean (1 = true, 0 = false)"),
              metric: Box::new(metric),
            })
            .await
          });
        }

        if let Some(state) = &*state {
          if let Some(true) = state.running {
            {
              let stats = Arc::clone(&stats);
              let labels = Arc::clone(&labels);
              let tx = Arc::clone(&tx);

              task::spawn(async move {
                let metric = Family::<ContainerMetricLabels, Gauge<f64, AtomicU64>>::default();
                metric.get_or_create(&labels).set(
                  Option::as_ref(&stats)
                    .and_then(|s| s.cpu_utilization())
                    .unwrap(),
                );
                tx.send(ContainerMetric {
                  name: String::from("cpu_utilization_percent"),
                  help: String::from("cpu utilization in percent"),
                  metric: Box::new(metric),
                })
                .await
              });
            }

            {
              let stats = Arc::clone(&stats);
              let labels = Arc::clone(&labels);
              let tx = Arc::clone(&tx);

              task::spawn(async move {
                let metric = Family::<ContainerMetricLabels, Gauge<f64, AtomicU64>>::default();
                metric.get_or_create(&labels).set(
                  Option::as_ref(&stats)
                    .and_then(|s| s.memory_usage())
                    .unwrap() as f64,
                );
                tx.send(ContainerMetric {
                  name: String::from("memory_usage_bytes"),
                  help: String::from("memory usage in bytes"),
                  metric: Box::new(metric),
                })
                .await
              });
            }

            {
              let stats = Arc::clone(&stats);
              let labels = Arc::clone(&labels);
              let tx = Arc::clone(&tx);

              task::spawn(async move {
                let metric = Family::<ContainerMetricLabels, Counter<f64, AtomicU64>>::default();
                metric.get_or_create(&labels).inner().set(
                  Option::as_ref(&stats)
                    .and_then(|s| s.memory_total())
                    .unwrap() as f64,
                );
                tx.send(ContainerMetric {
                  name: String::from("memory_bytes"),
                  help: String::from("memory total in bytes"),
                  metric: Box::new(metric),
                })
                .await
              });
            }

            {
              let stats = Arc::clone(&stats);
              let labels = Arc::clone(&labels);
              let tx = Arc::clone(&tx);

              task::spawn(async move {
                let metric = Family::<ContainerMetricLabels, Gauge<f64, AtomicU64>>::default();
                metric.get_or_create(&labels).set(
                  Option::as_ref(&stats)
                    .and_then(|s| s.memory_utilization())
                    .unwrap(),
                );
                tx.send(ContainerMetric {
                  name: String::from("memory_utilization_percent"),
                  help: String::from("memory utilization in percent"),
                  metric: Box::new(metric),
                })
                .await
              });
            }

            {
              let stats = Arc::clone(&stats);
              let labels = Arc::clone(&labels);
              let tx = Arc::clone(&tx);

              task::spawn(async move {
                let metric = Family::<ContainerMetricLabels, Counter<f64, AtomicU64>>::default();
                metric.get_or_create(&labels).inner().set(
                  Option::as_ref(&stats)
                    .and_then(|s| s.block_io_tx_total())
                    .unwrap() as f64,
                );
                tx.send(ContainerMetric {
                  name: String::from("block_io_tx_bytes"),
                  help: String::from("block io written total in bytes"),
                  metric: Box::new(metric),
                })
                .await
              });
            }

            {
              let stats = Arc::clone(&stats);
              let labels = Arc::clone(&labels);
              let tx = Arc::clone(&tx);

              task::spawn(async move {
                let metric = Family::<ContainerMetricLabels, Counter<f64, AtomicU64>>::default();
                metric.get_or_create(&labels).inner().set(
                  Option::as_ref(&stats)
                    .and_then(|s| s.block_io_rx_total())
                    .unwrap() as f64,
                );
                tx.send(ContainerMetric {
                  name: String::from("block_io_rx_bytes"),
                  help: String::from("block io read total in bytes"),
                  metric: Box::new(metric),
                })
                .await
              });
            }

            {
              let stats = Arc::clone(&stats);
              let labels = Arc::clone(&labels);
              let tx = Arc::clone(&tx);

              task::spawn(async move {
                let metric = Family::<ContainerMetricLabels, Counter<f64, AtomicU64>>::default();
                metric.get_or_create(&labels).inner().set(
                  Option::as_ref(&stats)
                    .and_then(|s| s.network_tx_total())
                    .unwrap() as f64,
                );
                tx.send(ContainerMetric {
                  name: String::from("network_tx_bytes"),
                  help: String::from("network sent total in bytes"),
                  metric: Box::new(metric),
                })
                .await
              });
            }

            {
              let stats = Arc::clone(&stats);
              let labels = Arc::clone(&labels);
              let tx = Arc::clone(&tx);

              task::spawn(async move {
                let metric = Family::<ContainerMetricLabels, Counter<f64, AtomicU64>>::default();
                metric.get_or_create(&labels).inner().set(
                  Option::as_ref(&stats)
                    .and_then(|s| s.network_rx_total())
                    .unwrap() as f64,
                );
                tx.send(ContainerMetric {
                  name: String::from("network_rx_bytes"),
                  help: String::from("network received total in bytes"),
                  metric: Box::new(metric),
                })
                .await
              });
            }
          }
        }
      });
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

        let (tx, mut rx) = mpsc::channel::<ContainerMetric<_>>(32);
        self.collect(docker, Arc::new(tx)).await;

        while let Some(ContainerMetric { name, help, metric }) = rx.recv().await {
          encoder
            .encode_descriptor(&name, &help, None, metric.metric_type())
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

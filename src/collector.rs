use bollard::{container, Docker};
use futures::StreamExt;
use prometheus_client::{
  encoding::EncodeLabelSet,
  metrics::{family::Family, gauge::Gauge},
  registry::Registry,
};
use std::sync::{Arc, Mutex};
use tokio;

pub struct DockerCollector {
  client: Arc<Docker>,
  registry: Arc<Mutex<Registry>>,
}

impl DockerCollector {
  pub fn new(client: Arc<Docker>, registry: Arc<Mutex<Registry>>) -> Arc<Self> {
    Arc::new(DockerCollector { client, registry })
  }

  pub async fn collect_metrics(self: Arc<Self>) {
    let options = Some(container::ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    });

    let containers = self
      .client
      .list_containers(options)
      .await
      .unwrap_or_default();

    for container in containers {
      let name = container
        .names
        .and_then(|names| Some(names.join(";")))
        .map(|names| names[1..].to_string())
        .unwrap_or_default();
      let running = container.state.eq(&Some(String::from("running")));
      let mut stats = self
        .client
        .stats(&container.id.unwrap_or_default(), Default::default())
        .take(1);

      let self_p = Arc::clone(&self);
      let name_p = Arc::new(name);

      tokio::spawn(async move {
        while let Some(Ok(stats)) = stats.next().await {
          let stats_p = Arc::new(stats);

          tokio::spawn(Arc::clone(&self_p).collect_state_metrics(
            Arc::clone(&name_p),
            Arc::clone(&stats_p),
            running,
          ));

          if running {
            tokio::spawn(
              Arc::clone(&self_p).collect_cpu_metrics(Arc::clone(&name_p), Arc::clone(&stats_p)),
            );
            tokio::spawn(
              Arc::clone(&self_p).collect_memory_metrics(Arc::clone(&name_p), Arc::clone(&stats_p)),
            );
            tokio::spawn(
              Arc::clone(&self_p).collect_io_metrics(Arc::clone(&name_p), Arc::clone(&stats_p)),
            );
            tokio::spawn(
              Arc::clone(&self_p)
                .collect_network_metrics(Arc::clone(&name_p), Arc::clone(&stats_p)),
            );
          }
        }
      });
    }
  }

  async fn collect_state_metrics(
    self: Arc<Self>,
    name: Arc<String>,
    stats: Arc<container::Stats>,
    running: bool,
  ) {
    // TODO: move struct to top of file
    #[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
    struct Labels {
      container_name: String,
    }

    let state_metric = Family::<Labels, Gauge>::default();
    state_metric
      .get_or_create(&Labels {
        container_name: name.to_string(),
      })
      .set(running as i64);

    // TODO: do not unwrap
    self.registry.lock().unwrap().register(
      "containers_running",
      "Containers running (1 = running, 0 = other)",
      state_metric,
    );
    println!("1. collecting state metrics");
  }

  async fn collect_cpu_metrics(self: Arc<Self>, name: Arc<String>, stats: Arc<container::Stats>) {
    println!("2. collecting cpu metrics");
  }

  async fn collect_memory_metrics(
    self: Arc<Self>,
    name: Arc<String>,
    stats: Arc<container::Stats>,
  ) {
    println!("3. collecting memory metrics");
  }

  async fn collect_io_metrics(self: Arc<Self>, name: Arc<String>, stats: Arc<container::Stats>) {
    println!("4. collecting io metrics");
  }

  async fn collect_network_metrics(
    self: Arc<Self>,
    name: Arc<String>,
    stats: Arc<container::Stats>,
  ) {
    println!("5. collecting network metrics");
  }
}

// TODO: tests

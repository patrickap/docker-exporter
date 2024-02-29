use bollard::{container, Docker};
use futures::StreamExt;
use std::sync::Arc;
use tokio;

pub struct DockerCollector {
  client: Docker,
}

impl DockerCollector {
  pub fn new(client: Docker) -> Arc<Self> {
    Arc::new(DockerCollector { client })
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
      let running = container.state.eq(&Some(String::from("running")));
      let mut stats = self
        .client
        .stats(&container.id.unwrap_or_default(), Default::default())
        .take(1);

      let self_p = Arc::clone(&self);

      tokio::spawn(async move {
        while let Some(Ok(stats)) = stats.next().await {
          let stats = Arc::new(stats);

          tokio::spawn(Arc::clone(&self_p).collect_state_metrics(running, Arc::clone(&stats)));

          if running {
            tokio::spawn(Arc::clone(&self_p).collect_cpu_metrics(Arc::clone(&stats)));
            tokio::spawn(Arc::clone(&self_p).collect_memory_metrics(Arc::clone(&stats)));
            tokio::spawn(Arc::clone(&self_p).collect_io_metrics(Arc::clone(&stats)));
            tokio::spawn(Arc::clone(&self_p).collect_network_metrics(Arc::clone(&stats)));
          }
        }
      });
    }
  }

  async fn collect_state_metrics(self: Arc<Self>, running: bool, stats: Arc<container::Stats>) {
    println!("1. collecting state metrics");
    println!("running: {}, stats: {:?}", running, stats)
  }

  async fn collect_cpu_metrics(self: Arc<Self>, stats: Arc<container::Stats>) {
    println!("2. collecting cpu metrics");
  }

  async fn collect_memory_metrics(self: Arc<Self>, stats: Arc<container::Stats>) {
    println!("3. collecting memory metrics");
  }

  async fn collect_io_metrics(self: Arc<Self>, stats: Arc<container::Stats>) {
    println!("4. collecting io metrics");
  }

  async fn collect_network_metrics(self: Arc<Self>, stats: Arc<container::Stats>) {
    println!("5. collecting network metrics");
  }
}

// TODO: tests

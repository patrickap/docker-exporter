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

  pub async fn collect(self: Arc<Self>) -> Result<(), bollard::errors::Error> {
    let options = Some(container::ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    });

    let containers = self.client.list_containers(options).await?;

    for container in containers {
      // TODO: do not unwrap
      let mut stats = self
        .client
        .stats(&container.id.unwrap().as_str(), Default::default())
        .take(1);
      let running = container.state.eq(&Some(String::from("running")));

      let self_clone = self.clone();
      tokio::spawn(async move {
        while let Some(Ok(stats)) = stats.next().await {
          self_clone
            .clone()
            .collect_metrics(Arc::new(stats), running)
            .await;
        }
      });
    }

    Ok(())
  }

  async fn collect_metrics(self: Arc<Self>, stats: Arc<container::Stats>, running: bool) {
    tokio::spawn(self.clone().collect_state_metrics(stats.clone(), running));

    if running {
      tokio::spawn(self.clone().collect_cpu_metrics(stats.clone()));
      tokio::spawn(self.clone().collect_memory_metrics(stats.clone()));
      tokio::spawn(self.clone().collect_io_metrics(stats.clone()));
      tokio::spawn(self.clone().collect_network_metrics(stats.clone()));
    }
  }

  async fn collect_state_metrics(self: Arc<Self>, stats: Arc<container::Stats>, running: bool) {
    println!("1. collecting state metrics");
    println!("stats: {:?}, running: {}", stats, running)
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

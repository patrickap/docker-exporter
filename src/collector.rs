use bollard;
use futures::StreamExt;
use tokio;

pub struct DockerCollector {
  api: bollard::Docker,
}

impl DockerCollector {
  pub fn new(api: bollard::Docker) -> std::sync::Arc<Self> {
    std::sync::Arc::new(DockerCollector { api })
  }

  pub async fn collect(self: std::sync::Arc<Self>) -> Result<(), bollard::errors::Error> {
    let options = Some(bollard::container::ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    });

    let containers = self.api.list_containers(options).await?;

    for container in containers {
      // TODO: do not unwrap
      let mut stats = self
        .api
        .stats(&container.id.unwrap().as_str(), Default::default())
        .take(1);
      let running = container.state.eq(&Some(String::from("running")));

      let self_clone = self.clone();
      tokio::spawn(async move {
        while let Some(Ok(stats)) = stats.next().await {
          self_clone
            .clone()
            .collect_metrics(std::sync::Arc::new(stats), running)
            .await;
        }
      });
    }

    Ok(())
  }

  async fn collect_metrics(
    self: std::sync::Arc<Self>,
    stats: std::sync::Arc<bollard::container::Stats>,
    running: bool,
  ) {
    tokio::spawn(self.clone().collect_state_metrics(stats.clone(), running));

    if running {
      tokio::spawn(self.clone().collect_cpu_metrics(stats.clone()));
      tokio::spawn(self.clone().collect_memory_metrics(stats.clone()));
      tokio::spawn(self.clone().collect_io_metrics(stats.clone()));
      tokio::spawn(self.clone().collect_network_metrics(stats.clone()));
    }
  }

  async fn collect_state_metrics(
    self: std::sync::Arc<Self>,
    stats: std::sync::Arc<bollard::container::Stats>,
    running: bool,
  ) {
    println!("1. collecting state metrics");
    println!("stats: {:?}, running: {}", stats, running)
  }

  async fn collect_cpu_metrics(
    self: std::sync::Arc<Self>,
    stats: std::sync::Arc<bollard::container::Stats>,
  ) {
    println!("2. collecting cpu metrics");
  }

  async fn collect_memory_metrics(
    self: std::sync::Arc<Self>,
    stats: std::sync::Arc<bollard::container::Stats>,
  ) {
    println!("3. collecting memory metrics");
  }

  async fn collect_io_metrics(
    self: std::sync::Arc<Self>,
    stats: std::sync::Arc<bollard::container::Stats>,
  ) {
    println!("4. collecting io metrics");
  }

  async fn collect_network_metrics(
    self: std::sync::Arc<Self>,
    stats: std::sync::Arc<bollard::container::Stats>,
  ) {
    println!("5. collecting network metrics");
  }
}

// TODO: tests

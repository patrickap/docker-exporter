use bollard;
use tokio::{self, join, try_join};

pub struct DockerCollector<'a> {
  api: &'a bollard::Docker,
}

impl<'a> DockerCollector<'a> {
  pub fn new(api: &'a bollard::Docker) -> Self {
    DockerCollector { api }
  }

  pub async fn collect(&self) -> Result<(), bollard::errors::Error> {
    let options = Some(bollard::container::ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    });

    let containers = self.api.list_containers(options).await?;

    let mut tasks = Vec::new();

    for container in containers {
      let running = container.state.eq(&Some(String::from("running")));

      tasks.push(tokio::task::spawn(
        self.collect_state_metrics(&container, running),
      ));

      if running {
        tasks.push(tokio::task::spawn(self.collect_cpu_metrics(&container)));
        tasks.push(tokio::task::spawn(self.collect_memory_metrics(&container)));
        tasks.push(tokio::task::spawn(self.collect_io_metrics(&container)));
        tasks.push(tokio::task::spawn(self.collect_network_metrics(&container)));
      }
    }

    let result = tokio::try_join!(tasks);

    Ok(())
  }

  async fn collect_state_metrics(
    &self,
    container: &bollard::models::ContainerSummary,
    running: bool,
  ) {
    println!("collecting state metrics");
    println!("id: {:?}, running: {}", container.id, running)
  }

  async fn collect_cpu_metrics(&self, container: &bollard::models::ContainerSummary) {
    println!("collecting cpu metrics");
  }

  async fn collect_memory_metrics(&self, container: &bollard::models::ContainerSummary) {
    println!("collecting memory metrics");
  }

  async fn collect_io_metrics(&self, container: &bollard::models::ContainerSummary) {
    println!("collecting io metrics");
  }

  async fn collect_network_metrics(&self, container: &bollard::models::ContainerSummary) {
    println!("collecting network metrics");
  }
}

// TODO: tests

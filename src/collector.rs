use bollard;

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

    for container in containers {
      // TODO: do concurrently
      let running = container.state.eq(&Some(String::from("running")));
      self.collectStateMetrics(&container, running).await;

      if running {
        self.collectCPUMetrics(&container).await;
        self.collectMemoryMetrics(&container).await;
        self.collectIOMetrics(&container).await;
        self.collectNetworkMetrics(&container).await;
      }
    }

    Ok(())
  }

  async fn collectStateMetrics(
    &self,
    container: &bollard::models::ContainerSummary,
    running: bool,
  ) {
    println!("collecting state metrics");
    println!("id: {:?}, running: {}", container.id, running)
  }

  async fn collectCPUMetrics(&self, container: &bollard::models::ContainerSummary) {
    println!("collecting cpu metrics");
  }

  async fn collectMemoryMetrics(&self, container: &bollard::models::ContainerSummary) {
    println!("collecting memory metrics");
  }

  async fn collectIOMetrics(&self, container: &bollard::models::ContainerSummary) {
    println!("collecting io metrics");
  }

  async fn collectNetworkMetrics(&self, container: &bollard::models::ContainerSummary) {
    println!("collecting network metrics");
  }
}

// TODO: tests

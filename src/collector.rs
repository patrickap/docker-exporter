use bollard;

pub struct DockerCollector<'a> {
  api: &'a bollard::Docker,
}

impl<'a> DockerCollector<'a> {
  pub fn new(api: &'a bollard::Docker) -> Self {
    DockerCollector { api }
  }

  pub async fn collect(&self) -> Result<(), bollard::errors::Error> {
    let options = Some(bollard::container::ListContainersOptions::<String> {
      all: true,
      ..Default::default()
    });

    let containers = self.api.list_containers(options).await?;

    for container in containers {
      // TODO: process each container async
      println!("{:?}", container)
    }

    Ok(())
  }
}

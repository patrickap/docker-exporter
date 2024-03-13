pub mod container;
pub mod metric;

use bollard::{
  container::{InspectContainerOptions, ListContainersOptions, Stats, StatsOptions},
  errors::Error,
  secret::{ContainerState, ContainerSummary},
  Docker,
};
use futures::stream::{StreamExt, TryStreamExt};
use std::env;

use crate::constant::{
  DOCKER_API_VERSION, DOCKER_CONNECTION_TIMEOUT, DOCKER_HOST_ENV, DOCKER_SOCKET_PATH,
};

pub trait DockerExt {
  fn try_connect() -> Result<Docker, Error>;
  async fn list_containers_all(&self) -> Option<Vec<ContainerSummary>>;
  async fn inspect_state(&self, container_name: &str) -> Option<ContainerState>;
  async fn stats_once(&self, container_name: &str) -> Option<Stats>;
  #[cfg(test)]
  fn try_connect_mock() -> Result<Docker, Error>;
}

impl DockerExt for Docker {
  fn try_connect() -> Result<Docker, Error> {
    match env::var(DOCKER_HOST_ENV) {
      Ok(docker_host) => {
        Docker::connect_with_http(&docker_host, DOCKER_CONNECTION_TIMEOUT, DOCKER_API_VERSION)
      }
      _ => Docker::connect_with_socket(
        DOCKER_SOCKET_PATH,
        DOCKER_CONNECTION_TIMEOUT,
        DOCKER_API_VERSION,
      ),
    }
  }

  async fn list_containers_all(&self) -> Option<Vec<ContainerSummary>> {
    let options = Some(ListContainersOptions::<&str> {
      all: true,
      ..Default::default()
    });

    self.list_containers(options).await.ok()
  }

  async fn inspect_state(&self, container_name: &str) -> Option<ContainerState> {
    let options = Some(InspectContainerOptions {
      ..Default::default()
    });

    self
      .inspect_container(container_name, options)
      .await
      .ok()
      .and_then(|inspect| inspect.state)
  }

  async fn stats_once(&self, container_name: &str) -> Option<Stats> {
    let options = Some(StatsOptions {
      stream: false,
      ..Default::default()
    });

    self
      .stats(container_name, options)
      .take(1)
      .try_next()
      .await
      .ok()
      .flatten()
  }

  #[cfg(test)]
  fn try_connect_mock() -> Result<Docker, Error> {
    // This is currently sufficient for a test as it returns Ok even if the socket is unavailable
    Docker::connect_with_socket("/dev/null", 0, DOCKER_API_VERSION)
  }
}

use bollard::{
  container::{InspectContainerOptions, ListContainersOptions, Stats, StatsOptions},
  errors::Error,
  secret::{ContainerState, ContainerSummary},
  Docker,
};
use futures::{StreamExt, TryStreamExt};
use std::env;

use crate::constant::{
  DOCKER_API_VERSION, DOCKER_CONNECTION_TIMEOUT, DOCKER_HOST_ENV, DOCKER_SOCKET_PATH,
};

pub trait DockerExt {
  fn try_connect() -> Result<Docker, Error>;
  async fn list_containers_all(&self) -> Option<Vec<ContainerSummary>>;
  async fn inspect_container_state(&self, container_name: &str) -> Option<ContainerState>;
  async fn stats_once(&self, container_name: &str) -> Option<Stats>;
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
    self
      .list_containers(Some(ListContainersOptions::<&str> {
        all: true,
        ..Default::default()
      }))
      .await
      .ok()
  }

  async fn inspect_container_state(&self, container_name: &str) -> Option<ContainerState> {
    self
      .inspect_container(
        container_name,
        Some(InspectContainerOptions {
          ..Default::default()
        }),
      )
      .await
      .ok()
      .and_then(|i| i.state)
  }

  async fn stats_once(&self, container_name: &str) -> Option<Stats> {
    self
      .stats(
        container_name,
        Some(StatsOptions {
          stream: false,
          ..Default::default()
        }),
      )
      .take(1)
      .try_next()
      .await
      .ok()
      .flatten()
  }
}

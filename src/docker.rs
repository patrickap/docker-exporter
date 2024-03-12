pub mod container;
pub mod metric;

use bollard::{errors::Error, Docker};
use std::env;

use crate::constants::{
  DOCKER_API_VERSION, DOCKER_CONNECTION_TIMEOUT, DOCKER_HOST_ENV, DOCKER_SOCKET_PATH,
};

pub fn connect() -> Result<Docker, Error> {
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

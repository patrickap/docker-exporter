pub mod container;
pub mod metric;

use bollard::{errors::Error, Docker};
use std::env;

use crate::constant::{
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

#[cfg(test)]
pub fn connect_mock() -> Result<Docker, Error> {
  // This is currently sufficient for a test as it returns Ok even if the socket is unavailable
  Docker::connect_with_socket("/dev/null", 0, DOCKER_API_VERSION)
}

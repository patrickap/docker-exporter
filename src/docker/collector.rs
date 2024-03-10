use bollard::{errors::Error, Docker};
use std::env;

use crate::constants::{
  DEFAULT_DOCKER_API_VERSION, DEFAULT_DOCKER_CONNECTION_TIMEOUT, DEFAULT_DOCKER_SOCKET_PATH,
  DOCKER_HOST_ENV,
};

pub struct Collector {}

impl Collector {
  pub fn connect() -> Result<Docker, Error> {
    match env::var(DOCKER_HOST_ENV) {
      Ok(docker_host) => Docker::connect_with_http(
        &docker_host,
        DEFAULT_DOCKER_CONNECTION_TIMEOUT,
        DEFAULT_DOCKER_API_VERSION,
      ),
      _ => Docker::connect_with_socket(
        DEFAULT_DOCKER_SOCKET_PATH,
        DEFAULT_DOCKER_CONNECTION_TIMEOUT,
        DEFAULT_DOCKER_API_VERSION,
      ),
    }
  }

  // pub fn collect() {}
  // fn get_containers() {}
  // fn get_container_info() {}
}

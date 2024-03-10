use bollard::{errors::Error, ClientVersion, Docker, API_DEFAULT_VERSION};
use prometheus_client::{
  encoding::EncodeLabelSet,
  metrics::{counter::Counter, family::Family, gauge::Gauge},
  registry::{Metric, Registry},
};
use std::{
  default, env,
  sync::atomic::{AtomicI64, AtomicU64},
};

const HOST_ENV: &str = "DOCKER_HOST";
const DEFAULT_SOCKET_PATH: &str = "/var/run/docker.sock";
const DEFAULT_API_VERSION: &ClientVersion = API_DEFAULT_VERSION;
const DEFAULT_CONNECTION_TIMEOUT: u64 = 60;

pub fn connect() -> Result<Docker, Error> {
  match env::var(HOST_ENV) {
    Ok(docker_host) => Docker::connect_with_http(
      &docker_host,
      DEFAULT_CONNECTION_TIMEOUT,
      DEFAULT_API_VERSION,
    ),
    _ => Docker::connect_with_socket(
      DEFAULT_SOCKET_PATH,
      DEFAULT_CONNECTION_TIMEOUT,
      DEFAULT_API_VERSION,
    ),
  }
}

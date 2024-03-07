pub mod constants {
  use bollard::{ClientVersion, API_DEFAULT_VERSION};

  pub const DOCKER_HOST_ENV: &str = "DOCKER_HOST";

  pub const DEFAULT_SOCKET_PATH: &str = "/var/run/docker.sock";
  pub const DEFAULT_API_VERSION: &ClientVersion = API_DEFAULT_VERSION;
  pub const DEFAULT_CONNECTION_TIMEOUT: u64 = 60;
}

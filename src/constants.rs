use bollard::{ClientVersion, API_DEFAULT_VERSION};

pub const SERVER_ADDRESS: &str = "0.0.0.0:9630";

pub const DOCKER_HOST_ENV: &str = "DOCKER_HOST";
pub const DEFAULT_DOCKER_SOCKET_PATH: &str = "/var/run/docker.sock";
pub const DEFAULT_DOCKER_API_VERSION: &ClientVersion = API_DEFAULT_VERSION;
pub const DEFAULT_DOCKER_CONNECTION_TIMEOUT: u64 = 60;

pub const PROMETHEUS_REGISTRY_PREFIX: &str = "docker_exporter";

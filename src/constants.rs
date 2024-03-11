use bollard::{ClientVersion, API_DEFAULT_VERSION};

pub const SERVER_ADDRESS: &str = "0.0.0.0:9630";

pub const PROMETHEUS_REGISTRY_PREFIX: &str = "docker_exporter";

pub const DOCKER_HOST_ENV: &str = "DOCKER_HOST";
pub const DOCKER_SOCKET_PATH: &str = "/var/run/docker.sock";
pub const DOCKER_API_VERSION: &ClientVersion = API_DEFAULT_VERSION;
pub const DOCKER_CONNECTION_TIMEOUT: u64 = 60;

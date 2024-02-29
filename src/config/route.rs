use bollard::Docker;

use crate::collector::DockerCollector;

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics() -> &'static str {
  // TODO: use http socket set via DOCKER_HOST env
  match Docker::connect_with_socket_defaults() {
    Ok(api) => {
      let metrics = DockerCollector::new(api).collect().await;
      // TODO: respond with metrics
    }
    Err(e) => {
      eprintln!("failed to connect to docker daemon: {}", e);
    }
  }
  "metrics"
}

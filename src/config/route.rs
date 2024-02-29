use axum::Extension;
use bollard::Docker;
use prometheus_client::registry::Registry;
use std::sync::Arc;

use crate::collector::DockerCollector;

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics(axum::Extension(registry): Extension<Arc<Registry>>) -> &'static str {
  // TODO: use http socket set via DOCKER_HOST env
  match Docker::connect_with_socket_defaults() {
    Ok(client) => {
      DockerCollector::new(client).collect_metrics(registry).await;
      // TODO: respond with metrics
    }
    Err(e) => {
      eprintln!("failed to connect to docker daemon: {}", e);
    }
  }
  "metrics"
}

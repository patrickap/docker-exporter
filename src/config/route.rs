use axum::Extension;
use bollard::Docker;
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::{Arc, Mutex};

use crate::collector::DockerCollector;

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics(axum::Extension(registry): Extension<Arc<Mutex<Registry>>>) -> String {
  // TODO: use http socket set via DOCKER_HOST env
  match Docker::connect_with_socket_defaults() {
    Ok(client) => {
      DockerCollector::new(Arc::new(client), Arc::clone(&registry))
        .collect_metrics()
        .await;
      // TODO: respond with metrics
    }
    Err(e) => {
      eprintln!("failed to connect to docker daemon: {}", e);
    }
  }

  // TODO: do not unwrap
  let mut encoded = String::new();
  text::encode(&mut encoded, &registry.lock().unwrap()).unwrap();
  encoded
}

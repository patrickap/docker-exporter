use axum::Extension;
use bollard::Docker;
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::{Arc, Mutex};

use crate::collector::DockerCollector;

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics(axum::Extension(registry): Extension<Arc<Mutex<Registry>>>) -> String {
  // TODO: do not unwrap
  let mut encoded = String::new();
  text::encode(&mut encoded, &registry.lock().unwrap()).unwrap();
  encoded
}

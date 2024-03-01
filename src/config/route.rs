use axum::Extension;
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::Arc;

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics(axum::Extension(registry): Extension<Arc<Registry>>) -> String {
  // TODO: do not unwrap
  let mut encoded = String::new();
  text::encode(&mut encoded, &registry).unwrap();
  encoded
}

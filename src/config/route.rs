use axum::{http, Extension};
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::Arc;

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics(
  axum::Extension(registry): Extension<Arc<Registry>>,
) -> Result<String, http::StatusCode> {
  let mut buffer = String::new();
  match text::encode(&mut buffer, &registry) {
    Ok(_) => Ok(buffer),
    _ => Err(http::StatusCode::INTERNAL_SERVER_ERROR),
  }
}

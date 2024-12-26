use axum::{http::StatusCode, response::IntoResponse, Extension};
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::Arc;

pub async fn status() -> Result<impl IntoResponse, StatusCode> {
  Ok((StatusCode::OK, "ok"))
}

pub async fn metrics(
  Extension(registry): Extension<Arc<Registry>>,
) -> Result<impl IntoResponse, StatusCode> {
  let mut buffer = String::new();
  match text::encode(&mut buffer, &registry) {
    Ok(_) => Ok((StatusCode::OK, buffer)),
    _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
  }
}

pub const SERVER_ADDRESS: &str = "0.0.0.0:9630";

pub mod route {
  use axum::{http::StatusCode, Extension};
  use prometheus_client::{encoding::text, registry::Registry};
  use std::sync::Arc;

  pub async fn status() -> &'static str {
    "ok"
  }

  pub async fn metrics(
    Extension(registry): Extension<Arc<Registry>>,
  ) -> Result<String, StatusCode> {
    let mut buffer = String::new();
    match text::encode(&mut buffer, &registry) {
      Ok(_) => Ok(buffer),
      _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
  }
}

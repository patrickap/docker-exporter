use axum::{http::StatusCode, Extension};
use bollard::Docker;
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::Arc;

use crate::docker::{
  container,
  metric::{self, Metrics},
};

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics<'a>(
  Extension(registry): Extension<Arc<Registry>>,
  Extension(docker): Extension<Arc<Docker>>,
  Extension(metrics): Extension<Arc<Metrics<'a>>>,
) -> Result<String, StatusCode> {
  let containers = container::collect(docker).await.unwrap_or_default();
  metric::update(Arc::clone(&metrics), containers);
  let mut buffer = String::new();
  match text::encode(&mut buffer, &registry) {
    Ok(_) => Ok(buffer),
    _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::docker::{self, metric::MetricsLabels};

  #[tokio::test]
  async fn it_returns_status() {
    let response = status().await;
    assert_eq!(response, "ok");
  }

  #[tokio::test]
  async fn it_returns_metrics() {
    let mut registry = Registry::from(Default::default());
    let docker = docker::connect_mock().unwrap();
    let metrics = metric::init();

    metrics.cpu_utilization_percent.register(&mut registry);

    metrics
      .cpu_utilization_percent
      .metric
      .get_or_create(&MetricsLabels {
        container_id: String::from("id_test"),
        container_name: String::from("name_test"),
      })
      .set(123.0);

    let response = super::metrics(
      Extension(Arc::new(registry)),
      Extension(Arc::new(docker)),
      Extension(Arc::new(metrics)),
    )
    .await;

    let expected = "# HELP cpu_utilization_percent cpu utilization in percent.\n".to_owned()
      + "# TYPE cpu_utilization_percent gauge\n"
      + "cpu_utilization_percent{container_id=\"id_test\",container_name=\"name_test\"} 123.0\n"
      + "# EOF\n";

    assert_eq!(response.is_ok(), true);
    assert_eq!(response, Ok(String::from(expected)));
  }
}

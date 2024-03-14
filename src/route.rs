use axum::{http::StatusCode, Extension};
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::Arc;

use crate::collector::DockerCollector;

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics<'a>(
  Extension(registry): Extension<Arc<Registry>>,
  Extension(collector): Extension<Arc<DockerCollector<'a>>>,
) -> Result<String, StatusCode> {
  let result = collector.collect().await.unwrap_or_default();

  for (state, stats) in result {
    collector.metrics.process(&state, &stats)
  }

  let mut buffer = String::new();
  match text::encode(&mut buffer, &registry) {
    Ok(_) => Ok(buffer),
    _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
  }
}

#[cfg(test)]
mod tests {
  use bollard::Docker;

  use super::*;
  use crate::{
    collector::{DockerLabels, DockerMetrics},
    constant::DOCKER_API_VERSION,
  };

  #[tokio::test]
  async fn it_returns_status() {
    let response = status().await;
    assert_eq!(response, "ok");
  }

  #[tokio::test]
  async fn it_returns_metrics() {
    let mut registry = Registry::from(Default::default());
    // This is currently sufficient for a test as it returns Ok even if the socket is unavailable
    let docker = Docker::connect_with_socket("/dev/null", 0, DOCKER_API_VERSION).unwrap();
    let metrics = DockerMetrics::new();

    metrics.cpu_utilization_percent.register(&mut registry);

    metrics
      .cpu_utilization_percent
      .metric
      .get_or_create(&DockerLabels {
        container_id: String::from("id_test"),
        container_name: String::from("name_test"),
      })
      .set(123.0);

    let collector = DockerCollector::new(docker, metrics);

    let response = super::metrics(
      Extension(Arc::new(registry)),
      Extension(Arc::new(collector)),
    )
    .await;

    let expected = "".to_owned()
      + "# HELP cpu_utilization_percent cpu utilization in percent.\n"
      + "# TYPE cpu_utilization_percent gauge\n"
      + "cpu_utilization_percent{container_id=\"id_test\",container_name=\"name_test\"} 123.0\n"
      + "# EOF\n";

    assert_eq!(response.is_ok(), true);
    assert_eq!(response, Ok(String::from(expected)));
  }
}

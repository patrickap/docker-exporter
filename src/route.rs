use axum::{http::StatusCode, response::IntoResponse, Extension};
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::Arc;

use crate::collector::{Collector, Metrics};

pub async fn status() -> Result<impl IntoResponse, StatusCode> {
  Ok((StatusCode::OK, "ok"))
}

pub async fn metrics<C: Collector, M: Metrics<C>>(
  Extension(registry): Extension<Arc<Registry>>,
  Extension(collector): Extension<Arc<C>>,
  Extension(metrics): Extension<Arc<M>>,
) -> Result<impl IntoResponse, StatusCode> {
  let output = collector.collect().await.unwrap_or_default();
  metrics.process(output.into());

  let mut buffer = String::new();
  match text::encode(&mut buffer, &registry) {
    Ok(_) => Ok((StatusCode::OK, buffer)),
    _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
  }
}

#[cfg(test)]
mod tests {
  use axum::body::{self, Body};
  use bollard::Docker;
  use std::error::Error;

  use super::*;
  use crate::{
    collector::{DockerCollector, DockerMetricLabels, DockerMetrics, Metric},
    extension::DockerExt,
  };

  async fn read_body_as_str(body: Body) -> Result<String, Box<dyn Error>> {
    let bytes = body::to_bytes(body, usize::MAX).await?;
    Ok(String::from_utf8(bytes.to_vec())?)
  }

  #[tokio::test]
  async fn it_returns_status() {
    let result = status().await;
    assert!(result.is_ok());

    let response = result.unwrap().into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = read_body_as_str(response.into_body()).await.unwrap();

    assert_eq!(body, "ok");
  }

  #[tokio::test]
  async fn it_returns_metrics() {
    let mut registry = Registry::from(Default::default());
    let docker = Docker::try_connect_mock().unwrap();
    let collector = DockerCollector::new(Arc::new(docker));
    let metrics = DockerMetrics::new();

    metrics.cpu_utilization_percent.register(&mut registry);

    metrics
      .cpu_utilization_percent
      .metric
      .get_or_create(&DockerMetricLabels {
        container_id: String::from("id_test"),
        container_name: String::from("name_test"),
      })
      .set(123.0);

    let result = super::metrics(
      Extension(Arc::new(registry)),
      Extension(Arc::new(collector)),
      Extension(Arc::new(metrics)),
    )
    .await;

    assert!(result.is_ok());

    let response = result.unwrap().into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = read_body_as_str(response.into_body()).await.unwrap();

    let expected = "".to_owned()
      + "# HELP cpu_utilization_percent cpu utilization in percent.\n"
      + "# TYPE cpu_utilization_percent gauge\n"
      + "cpu_utilization_percent{container_id=\"id_test\",container_name=\"name_test\"} 123.0\n"
      + "# EOF\n";

    assert_eq!(body, expected);
  }
}

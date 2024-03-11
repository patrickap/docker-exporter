use axum::{http::StatusCode, Extension};
use bollard::Docker;
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::Arc;

use crate::docker::metrics::MetricsCollect;

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics<M: MetricsCollect<Collector = Docker>>(
  Extension(registry): Extension<Arc<Registry>>,
  Extension(docker): Extension<Arc<Docker>>,
  Extension(metrics): Extension<Arc<M>>,
) -> Result<String, StatusCode> {
  metrics
    .collect_metrics(Arc::clone(&docker))
    .await
    .or(Err(StatusCode::INTERNAL_SERVER_ERROR))?;

  let mut buffer = String::new();
  match text::encode(&mut buffer, &registry) {
    Ok(_) => Ok(buffer),
    _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::docker::metrics::Metrics;
  use std::error::Error;

  #[tokio::test]
  async fn it_returns_status() {
    let result = status().await;
    assert_eq!(result, "ok");
  }

  #[tokio::test]
  async fn it_returns_metrics_ok() {
    let r = Registry::from(Default::default());
    let d = Docker::connect_with_socket_defaults().unwrap();
    let m = Metrics::new();

    let result = metrics(
      Extension(Arc::new(r)),
      Extension(Arc::new(d)),
      Extension(Arc::new(m)),
    )
    .await;

    assert_eq!(result.is_ok(), true);
    assert_eq!(result, Ok(String::from("# EOF\n")));
  }

  #[tokio::test]
  async fn it_returns_metrics_err() {
    #[derive(Debug)]
    struct MockMetrics {}

    impl MetricsCollect for MockMetrics {
      type Collector = Docker;

      async fn collect_metrics(&self, _: Arc<Self::Collector>) -> Result<(), Box<dyn Error>> {
        Err(Box::new(std::fmt::Error {}))
      }
    }

    let d = Docker::connect_with_socket_defaults().unwrap();
    let r = Registry::from(Default::default());
    let m = MockMetrics {};

    let result = metrics(
      Extension(Arc::new(r)),
      Extension(Arc::new(d)),
      Extension(Arc::new(m)),
    )
    .await;

    assert_eq!(result.is_err(), true);
    assert_eq!(result, Err(StatusCode::INTERNAL_SERVER_ERROR));
  }
}

use axum::{http::StatusCode, Extension};
use bollard::Docker;
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::Arc;

use crate::docker::{container, metrics::Metrics};

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics(
  Extension(docker): Extension<Arc<Docker>>,
  Extension(registry): Extension<Arc<Registry>>,
  Extension(metrics): Extension<Arc<Metrics>>,
) -> Result<String, StatusCode> {
  container::collect_metrics(Arc::clone(&docker), Arc::clone(&metrics))
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
  use std::fmt::Error;

  use prometheus_client::{collector::Collector, encoding::DescriptorEncoder};

  use super::*;

  #[tokio::test]
  async fn it_returns_status() {
    let result = status().await;
    assert_eq!(result, "ok");
  }

  #[tokio::test]
  async fn it_returns_metrics_ok() {
    let d = Docker::connect_with_socket_defaults().unwrap();
    let r = Registry::from(Default::default());
    let m = Metrics::new();

    let result = metrics(
      Extension(Arc::new(d)),
      Extension(Arc::new(r)),
      Extension(Arc::new(m)),
    )
    .await;

    assert_eq!(result.is_ok(), true);
    assert_eq!(result, Ok(String::from("# EOF\n")));
  }

  #[tokio::test]
  async fn it_returns_metrics_err() {
    #[derive(Debug)]
    struct ErrCollector {}

    impl Collector for ErrCollector {
      fn encode(&self, _: DescriptorEncoder) -> Result<(), Error> {
        Err(Default::default())
      }
    }

    let d = Docker::connect_with_socket_defaults().unwrap();
    let mut r = Registry::from(Default::default());
    let m = Metrics::new();

    r.register_collector(Box::new(ErrCollector {}));

    let result = metrics(
      Extension(Arc::new(d)),
      Extension(Arc::new(r)),
      Extension(Arc::new(m)),
    )
    .await;

    assert_eq!(result.is_err(), true);
    assert_eq!(result, Err(StatusCode::INTERNAL_SERVER_ERROR));
  }
}

use axum::{http::StatusCode, Extension};
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::Arc;

pub async fn status() -> &'static str {
  "ok"
}

pub async fn metrics(Extension(registry): Extension<Arc<Registry>>) -> Result<String, StatusCode> {
  let mut buffer = String::new();
  match text::encode(&mut buffer, &registry) {
    Ok(_) => Ok(buffer),
    _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use prometheus_client::{collector::Collector, encoding::DescriptorEncoder};
  use std::fmt::Error;

  #[tokio::test]
  async fn it_returns_status() {
    let status = status().await;
    assert_eq!(status, "ok");
  }

  #[tokio::test]
  async fn it_returns_metrics_ok() {
    let registry = Registry::from(Default::default());
    let metrics = metrics(Extension(Arc::new(registry))).await;

    assert_eq!(metrics.is_ok(), true);
    assert_eq!(metrics, Ok(String::from("# EOF\n")));
  }

  #[tokio::test]
  async fn it_returns_metrics_err() {
    #[derive(Debug)]
    struct MockCollector {
      result: Result<(), Error>,
    }

    impl Collector for MockCollector {
      fn encode(&self, _: DescriptorEncoder) -> Result<(), Error> {
        self.result
      }
    }

    let mut registry = Registry::from(Default::default());
    registry.register_collector(Box::new(MockCollector {
      result: Err(Default::default()),
    }));

    let metrics = metrics(Extension(Arc::new(registry))).await;

    assert_eq!(metrics.is_err(), true);
    assert_eq!(metrics, Err(StatusCode::INTERNAL_SERVER_ERROR));
  }
}

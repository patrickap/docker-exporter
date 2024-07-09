use axum::{http::StatusCode, response::IntoResponse, Extension};
use prometheus_client::{encoding::text, registry::Registry};
use std::sync::Arc;

use crate::collector::{Collector, Metrics};

pub async fn status() -> Result<impl IntoResponse, StatusCode> {
  Ok((StatusCode::OK, "ok"))
}

pub async fn metrics<C, M>(
  Extension(registry): Extension<Arc<Registry>>,
  Extension(collector): Extension<Arc<C>>,
  Extension(metrics): Extension<Arc<M>>,
) -> Result<impl IntoResponse, StatusCode>
where
  C: Collector,
  M: Metrics,
  <M as Metrics>::Input: From<<C as Collector>::Output>,
{
  let result = collector.collect().await;
  metrics.clear();
  metrics.update(result.into());

  let mut buffer = String::new();
  match text::encode(&mut buffer, &registry) {
    Ok(_) => Ok((StatusCode::OK, buffer)),
    _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
  }
}

#[cfg(test)]
mod tests {
  use axum::body::{self, Body};
  use prometheus_client::metrics::{counter::Counter, family::Family, gauge::Gauge};
  use std::{error::Error, sync::atomic::AtomicU64};

  use super::*;
  use crate::{collector::Metric, extension::RegistryExt};

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

    struct MockCollector {}

    impl Collector for MockCollector {
      type Output = i32;
      async fn collect(&self) -> Self::Output {
        123
      }
    }

    struct MockMetrics {
      gauge_metric_test: Metric<Family<Vec<(String, String)>, Gauge<f64, AtomicU64>>>,
      counter_metric_test: Metric<Family<Vec<(String, String)>, Counter<f64, AtomicU64>>>,
    }

    impl Metrics for MockMetrics {
      type Origin = MockCollector;
      type Input = <Self::Origin as Collector>::Output;
      fn register(&self, registry: &mut Registry) {
        registry.register_metric(&self.gauge_metric_test);
        registry.register_metric(&self.counter_metric_test);
      }
      fn clear(&self) {
        self.gauge_metric_test.metric.clear();
        self.counter_metric_test.metric.clear();
      }
      fn update(&self, input: Self::Input) {
        self
          .gauge_metric_test
          .metric
          .get_or_create(&Vec::from([(
            "gauge_label_key_test".to_owned(),
            "gauge_label_value_test".to_owned(),
          )]))
          .set(input as f64);
        self
          .counter_metric_test
          .metric
          .get_or_create(&Vec::from([(
            "counter_label_key_test".to_owned(),
            "counter_label_value_test".to_owned(),
          )]))
          .inc();
      }
    }

    let collector = MockCollector {};

    let metrics = MockMetrics {
      gauge_metric_test: Metric::new(
        "gauge_metric_test",
        "gauge_metric_help_test",
        Default::default(),
      ),
      counter_metric_test: Metric::new(
        "counter_metric_test",
        "counter_metric_help_test",
        Default::default(),
      ),
    };

    metrics.register(&mut registry);

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
      + "# HELP gauge_metric_test gauge_metric_help_test.\n"
      + "# TYPE gauge_metric_test gauge\n"
      + "gauge_metric_test{gauge_label_key_test=\"gauge_label_value_test\"} 123.0\n"
      + "# HELP counter_metric_test counter_metric_help_test.\n"
      + "# TYPE counter_metric_test counter\n"
      + "counter_metric_test_total{counter_label_key_test=\"counter_label_value_test\"} 1.0\n"
      + "# EOF\n";

    assert_eq!(body, expected);
  }
}

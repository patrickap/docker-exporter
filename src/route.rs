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

#[cfg(test)]
mod tests {
  use axum::body::{self, Body};
  use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::{counter::ConstCounter, gauge::ConstGauge},
  };
  use std::error::Error;

  use super::*;

  #[tokio::test]
  async fn it_returns_status_response() {
    let result = status().await;
    assert!(result.is_ok());

    let response = result.unwrap().into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = read_body_as_str(response.into_body()).await.unwrap();
    assert_eq!(body, "ok");
  }

  #[tokio::test]
  async fn it_returns_metrics_response() {
    #[derive(Debug)]
    struct MockCollector {}

    impl Collector for MockCollector {
      fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        {
          let gauge = ConstGauge::new(1.0 as f64);
          let gauge_encoder = encoder.encode_descriptor(
            "const_gauge_name",
            "const_gauge_help",
            None,
            gauge.metric_type(),
          )?;
          gauge.encode(gauge_encoder)?;
        }

        {
          let counter = ConstCounter::new(1.0 as f64);
          let counter_encoder = encoder.encode_descriptor(
            "const_counter_name",
            "const_counter_help",
            None,
            counter.metric_type(),
          )?;
          counter.encode(counter_encoder)?;
        }

        Ok(())
      }
    }

    let collector = MockCollector {};

    let mut registry = Registry::default();
    registry.register_collector(Box::new(collector));

    let result = metrics(Extension(Arc::new(registry))).await;

    assert!(result.is_ok());

    let response = result.unwrap().into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = read_body_as_str(response.into_body()).await.unwrap();

    let expected = "".to_owned()
      + "# HELP const_gauge_name const_gauge_help\n"
      + "# TYPE const_gauge_name gauge\n"
      + "const_gauge_name 1.0\n"
      + "# HELP const_counter_name const_counter_help\n"
      + "# TYPE const_counter_name counter\n"
      + "const_counter_name_total 1.0\n"
      + "# EOF\n";

    assert_eq!(body, expected);
  }

  async fn read_body_as_str(body: Body) -> Result<String, Box<dyn Error>> {
    let bytes = body::to_bytes(body, usize::MAX).await?;
    Ok(String::from_utf8(bytes.to_vec())?)
  }
}

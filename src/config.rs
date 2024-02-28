use super::collector;

pub mod consts {
  pub const ADDRESS: &str = "0.0.0.0:9630";
}

pub mod routes {
  use bollard;

  pub async fn status() -> &'static str {
    "ok"
  }

  pub async fn metrics() -> &'static str {
    match bollard::Docker::connect_with_http_defaults() {
      Ok(api) => {
        let metrics = super::collector::DockerCollector::new(&api).collect().await;
        // TODO: respond with metrics
      }
      Err(e) => {
        eprintln!("failed to connect to docker daemon: {}", e);
      }
    }
    "metrics"
  }
}

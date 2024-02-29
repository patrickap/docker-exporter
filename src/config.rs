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
    // TODO: use http socket set via DOCKER_HOST env
    match bollard::Docker::connect_with_socket_defaults() {
      Ok(api) => {
        let metrics = super::collector::DockerCollector::new(api).collect().await;
        // TODO: respond with metrics
      }
      Err(e) => {
        eprintln!("failed to connect to docker daemon: {}", e);
      }
    }
    "metrics"
  }
}

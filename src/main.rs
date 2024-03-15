mod collector;
mod constant;
mod extension;
mod route;

use axum::{routing, Extension, Router};
use bollard::Docker;
use prometheus_client::registry::Registry;
use std::{error::Error, sync::Arc};
use tokio::{net::TcpListener, signal};

use crate::{
  collector::{Collector, DockerCollector, DockerMetrics, Metric, Metrics},
  constant::{PROMETHEUS_REGISTRY_PREFIX, SERVER_ADDRESS},
  extension::DockerExt,
};

// TODO: check again metrics calculation, names etc.
// TODO: tests
// TODO: add timeout to graceful shutdown
// TODO: use impl Error instead of Box<dyn Error>?

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let mut registry = Registry::with_prefix(PROMETHEUS_REGISTRY_PREFIX);

  let docker = match Docker::try_connect() {
    Ok(docker) => Ok(docker),
    Err(err) => {
      eprintln!("failed to connect to docker daemon: {:?}", err);
      Err(err)
    }
  }?;

  let collector = DockerCollector::new(Arc::new(docker));

  let metrics = DockerMetrics::new();
  metrics.state_running_boolean.register(&mut registry);
  metrics.cpu_utilization_percent.register(&mut registry);
  metrics.memory_usage_bytes.register(&mut registry);
  metrics.memory_bytes_total.register(&mut registry);
  metrics.memory_utilization_percent.register(&mut registry);
  metrics.block_io_tx_bytes_total.register(&mut registry);
  metrics.block_io_rx_bytes_total.register(&mut registry);
  metrics.network_tx_bytes_total.register(&mut registry);
  metrics.network_rx_bytes_total.register(&mut registry);

  let listener = TcpListener::bind(SERVER_ADDRESS).await?;
  let router = Router::new()
    .route("/status", routing::get(route::status))
    .route(
      "/metrics",
      routing::get(route::metrics::<DockerCollector<_>, DockerMetrics, Arc<Docker>>),
    )
    .layer(Extension(Arc::new(registry)))
    .layer(Extension(Arc::new(collector)))
    .layer(Extension(Arc::new(metrics)));

  println!("server listening on {}", listener.local_addr()?);
  axum::serve(listener, router)
    .with_graceful_shutdown(async {
      if let Ok(_) = signal::ctrl_c().await {
        println!("\nreceived signal; shutting down");
      }
    })
    .await?;

  println!("server stopped");

  Ok(())
}

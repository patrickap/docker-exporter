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
  collector::{Collector, DockerCollector, DockerMetrics, Metrics},
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

  let metrics = DockerMetrics::new();
  metrics.register(&mut registry);

  let collector = DockerCollector::new(Arc::new(docker), Arc::new(metrics));

  let listener = TcpListener::bind(SERVER_ADDRESS).await?;
  let router = Router::new()
    .route("/status", routing::get(route::status))
    .route(
      "/metrics",
      routing::get(route::metrics::<DockerCollector<Docker, DockerMetrics>>),
    )
    .layer(Extension(Arc::new(registry)))
    .layer(Extension(Arc::new(collector)));

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

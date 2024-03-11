mod constants;
mod docker;
mod routes;

use axum::{routing, Extension, Router};
use prometheus_client::registry::Registry;
use std::{error::Error, sync::Arc};
use tokio::{net::TcpListener, signal};

use crate::constants::{PROMETHEUS_REGISTRY_PREFIX, SERVER_ADDRESS};
use crate::docker::metrics::Metrics;

// TODO: check again metrics calculation, names etc.
// TODO: http header for open metrics text?
// TODO: tests
// TODO: add timeout to graceful shutdown

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let docker = docker::connect().map_err(|err| {
    eprintln!("failed to connect to docker daemon: {:?}", err);
    err
  })?;

  let mut registry = Registry::with_prefix(PROMETHEUS_REGISTRY_PREFIX);

  let metrics = Metrics::new();
  metrics.register(&mut registry);

  let listener = TcpListener::bind(SERVER_ADDRESS).await?;
  let router = Router::new()
    .route("/status", routing::get(routes::status))
    .route("/metrics", routing::get(routes::metrics))
    .layer(Extension(Arc::new(docker)))
    .layer(Extension(Arc::new(registry)))
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

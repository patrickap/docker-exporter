use axum::{routing, Extension, Router};
use prometheus_client::registry::Registry;
use std::error::Error;
use std::sync::Arc;
use tokio::{net::TcpListener, signal};

mod constants;
mod docker;
mod routes;

use crate::constants::{PROMETHEUS_REGISTRY_PREFIX, SERVER_ADDRESS};
use crate::docker::{collector::Collector, metrics::Metrics};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let docker = Collector::connect().map_err(|err| {
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
      // TODO: add timeout
      if let Ok(_) = signal::ctrl_c().await {
        println!("\nreceived signal; shutting down");
      }
    })
    .await?;

  println!("server stopped");

  Ok(())
}

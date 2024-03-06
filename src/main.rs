use axum::{routing, Router};
use prometheus_client::registry::Registry;
use std::{io, sync::Arc};
use tokio::{net::TcpListener, signal};

mod collector;
mod config;

use crate::collector::{Collector, DockerCollector};
use crate::config::{registry, route, server};

// TODO: refactor imports
// TODO: create docker image
// TODO: support default docker socket path and / or tcp

#[tokio::main]
async fn main() -> io::Result<()> {
  let mut registry = Registry::with_prefix(registry::PREFIX);
  registry.register_collector(Box::new(DockerCollector::new()));

  let listener = TcpListener::bind(server::ADDRESS).await?;
  let router = Router::new()
    .route("/status", routing::get(route::status))
    .route("/metrics", routing::get(route::metrics))
    .layer(axum::Extension(Arc::new(registry)));

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

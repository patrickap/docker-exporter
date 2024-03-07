use axum::{routing, Extension, Router};
use prometheus_client::registry::Registry;
use std::{io::Result, sync::Arc};
use tokio::{net::TcpListener, signal};

mod collector;
mod config;

use crate::collector::{DefaultCollector, DockerCollector};
use crate::config::{
  axum::{constants::SERVER_ADDRESS, routes},
  prometheus::constants::REGISTRY_PREFIX,
};

// TODO: create docker image
// TODO: tests

#[tokio::main]
async fn main() -> Result<()> {
  let mut registry = Registry::with_prefix(REGISTRY_PREFIX);
  registry.register_collector(Box::new(DockerCollector::new()));

  let listener = TcpListener::bind(SERVER_ADDRESS).await?;
  let router = Router::new()
    .route("/status", routing::get(routes::status))
    .route("/metrics", routing::get(routes::metrics))
    .layer(Extension(Arc::new(registry)));

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

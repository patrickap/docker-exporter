use axum::{routing, Router};
use bollard::Docker;
use prometheus_client::registry::Registry;
use std::{
  io,
  sync::{Arc, Mutex},
};
use tokio::{net::TcpListener, signal};

mod collector;
mod config;

use crate::collector::DockerCollector;
use crate::config::{registry, route, server};

#[tokio::main]
async fn main() -> io::Result<()> {
  let mut registry = Registry::with_prefix(registry::PREFIX);
  // TODO: do not unwrap, use http socket
  registry.register_collector(Box::new(DockerCollector::new(
    Docker::connect_with_socket_defaults().unwrap(),
  )));

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

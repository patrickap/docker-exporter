use axum::{routing, Router};
use prometheus_client::registry::Registry;
use std::{io, sync::Arc};
use tokio::{net::TcpListener, signal};

mod collector;
mod config;

use crate::config::{route, server};

#[tokio::main]
async fn main() -> io::Result<()> {
  let registry = Arc::new(Registry::with_prefix("docker_exporter"));
  let listener = TcpListener::bind(server::ADDRESS).await?;
  let router = Router::new()
    .route("/status", routing::get(route::status))
    .route("/metrics", routing::get(route::metrics))
    .layer(axum::Extension(registry));

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

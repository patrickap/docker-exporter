use axum::{routing, Extension, Router};
use std::{io::Result, sync::Arc};
use tokio::{net::TcpListener, signal};

mod collector;
mod config;

use crate::collector::DockerCollector;
use crate::config::{constants::SERVER_ADDRESS, routes};

#[tokio::main]
async fn main() -> Result<()> {
  let collector = DockerCollector::new();

  let listener = TcpListener::bind(SERVER_ADDRESS).await?;
  let router = Router::new()
    .route("/status", routing::get(routes::status))
    .route("/metrics", routing::get(routes::metrics))
    .layer(Extension(Arc::new(collector)));

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

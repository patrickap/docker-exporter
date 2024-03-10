use axum::{routing, Extension, Router};
use std::error::Error;
use std::sync::Arc;
use tokio::{net::TcpListener, signal};

mod config;
mod docker;

use crate::config::{constants::SERVER_ADDRESS, routes};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let docker = docker::connect().map_err(|err| {
    eprintln!("failed to connect to docker daemon: {:?}", err);
    err
  })?;

  let listener = TcpListener::bind(SERVER_ADDRESS).await?;
  let router = Router::new()
    .route("/status", routing::get(routes::status))
    .route("/metrics", routing::get(routes::metrics))
    .layer(Extension(Arc::new(docker)));

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

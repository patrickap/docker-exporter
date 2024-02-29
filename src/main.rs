use axum::{routing, Router};
use std::io;
use tokio::{net::TcpListener, signal};

mod collector;
mod config;

use crate::config::{route, server};

#[tokio::main]
async fn main() -> io::Result<()> {
  let listener = TcpListener::bind(server::ADDRESS).await?;
  let router = Router::new()
    .route("/status", routing::get(route::status))
    .route("/metrics", routing::get(route::metrics));

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

use axum;
use std;
use tokio;

mod config;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let listener = tokio::net::TcpListener::bind(config::consts::ADDRESS).await?;
  let router = axum::Router::new()
    .route("/status", axum::routing::get(config::routes::status))
    .route("/metrics", axum::routing::get(config::routes::metrics));

  println!("server listening on {}", listener.local_addr()?);
  axum::serve(listener, router)
    .with_graceful_shutdown(async {
      if let Ok(_) = tokio::signal::ctrl_c().await {
        println!("\nserver stopped");
      }
    })
    .await?;

  Ok(())
}

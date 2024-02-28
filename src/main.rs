use axum;
use std;
use tokio;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let listener = tokio::net::TcpListener::bind("0.0.0.0:9630").await?;
  let router = axum::Router::new().route("/metrics", axum::routing::get(metrics));

  println!("server listening on {:?}", listener.local_addr()?);
  axum::serve(listener, router).await?;

  println!("server stopped");
  Ok(())
}

async fn metrics() -> &'static str {
  "Hello, World!"
}

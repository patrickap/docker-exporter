use axum;
use std;
use tokio;

mod server;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let address = "0.0.0.0:1234";
  let routes = Vec::from([server::Route {
    path: "/metrics",
    handler: axum::routing::get(|| async { "Hello World!" }),
  }]);

  server::run(address, routes).await?;
  Ok(())
}

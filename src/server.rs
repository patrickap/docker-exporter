use axum;
use std;
use tokio;

pub struct Route<'a> {
  pub path: &'a str,
  pub handler: axum::routing::MethodRouter<()>,
}

pub async fn run<'a>(address: &str, routes: Vec<Route<'a>>) -> std::io::Result<()> {
  let listener = tokio::net::TcpListener::bind(address).await?;
  let router = routes
    .into_iter()
    .fold(axum::Router::new(), |router, route| {
      router.route(route.path, route.handler)
    });

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

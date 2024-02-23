use std::{error::Error, sync::Arc};

use tiny_http;
use tokio;

mod config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
  let server = Arc::new(tiny_http::Server::http(config::SERVER_ADDRESS)?);
  let server_ref = Arc::clone(&server);
  println!("server listening on {}", config::SERVER_ADDRESS);

  // TODO: without move possible?
  let signals = tokio::spawn(signals(move || server_ref.unblock()));

  let mut tasks = tokio::task::JoinSet::new();
  let tasks = tokio::spawn(async move {
    for request in server.incoming_requests() {
      tasks.spawn_blocking(|| {
        handle_request(request);
      });
    }
  });

  tokio::try_join!(signals, tasks)?;
  println!("shutting down server");
  Ok(())
}

fn handle_request(request: tiny_http::Request) {
  let result = match request.url() {
    "/metrics" => request.respond(tiny_http::Response::from_string(
      "metrics can be viewed here",
    )),
    _ => request.respond(tiny_http::Response::from_string("404")),
  };

  if let Err(e) = result {
    eprintln!("failed to handle request: {e}")
  }
}

#[cfg(unix)]
async fn signals(cleanup: impl FnOnce() -> ()) {
  // TODO: handle unwrap()
  let mut sigint =
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
  let mut sigterm =
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

  tokio::select! {
    _ = sigint.recv() => {
      println!("received SIGINT");
      cleanup()
    },
    _ = sigterm.recv() => {
      println!("received SIGTERM");
      cleanup()
    },
  }
}

#[cfg(windows)]
async fn signals(cleanup: impl FnOnce() -> ()) {
  // TODO: handle windows signals
}

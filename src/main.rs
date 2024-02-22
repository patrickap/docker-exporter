use std::{
  error::Error,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
};
use tiny_http::{Request, Response, Server};

mod common;
mod config;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
  let signal = Arc::new(AtomicBool::new(false));
  signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&signal))?;
  signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&signal))?;

  let server = Arc::new(Server::http(config::SERVER_ADDRESS)?);
  let server_workers = common::ThreadPool::new(config::SERVER_WORKERS);
  println!("server listening on {}", config::SERVER_ADDRESS);

  while !signal.load(Ordering::Relaxed) {
    if let Ok(Some(request)) = server.recv_timeout(config::SERVER_TIMEOUT) {
      server_workers.execute(|| {
        handle_request(request);
      });
    }
  }

  println!("shutting down server");
  Ok(())
}

fn handle_request(request: Request) {
  let result = match request.url() {
    "/metrics" => request.respond(Response::from_string("metrics can be viewed here")),
    _ => request.respond(Response::from_string("404")),
  };

  if let Err(e) = result {
    eprintln!("failed to handle request: {e}")
  }
}

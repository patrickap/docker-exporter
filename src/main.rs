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
  let running = Arc::new(AtomicBool::new(true));
  let r = Arc::clone(&running);

  ctrlc::set_handler(move || {
    r.store(false, Ordering::SeqCst);
  })?;

  let server = Arc::new(Server::http(config::SERVER_ADDRESS)?);
  let server_workers = common::ThreadPool::new(config::SERVER_WORKERS);
  println!("server listening on {}", config::SERVER_ADDRESS);

  while running.load(Ordering::SeqCst) {
    if let Ok(Some(request)) = server.try_recv() {
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

use std::{
  error::Error,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  time::Duration,
};
use tiny_http::{Request, Response, Server};

mod server;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
  let running = Arc::new(AtomicBool::new(true));
  let r = Arc::clone(&running);

  ctrlc::set_handler(move || {
    r.store(false, Ordering::SeqCst);
  })?;

  let address = "0.0.0.0:9632";
  let server = Arc::new(Server::http(address)?);
  println!("server listening on {address}");

  let thread_pool = server::ThreadPool::new(4);

  while running.load(Ordering::SeqCst) {
    if let Ok(Some(request)) = server.try_recv() {
      thread_pool.execute(|| {
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

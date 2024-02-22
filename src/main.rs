use std::{error::Error, sync::Arc};
use tiny_http::{Request, Response, Server};

mod server;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
  let server = Arc::new(Server::http("0.0.0.0:9630")?);
  let thread_pool = server::ThreadPool::new(4);

  for request in server.incoming_requests() {
    thread_pool.execute(|| {
      handle_request(request);
    })
  }

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

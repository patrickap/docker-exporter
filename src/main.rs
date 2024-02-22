use std::{error::Error, io::Error as IoError, sync::Arc};
use tiny_http::{Request, Response, Server};

mod server;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
  let server = Arc::new(Server::http("0.0.0.0:9630")?);
  let thread_pool = server::ThreadPool::new(4);

  for request in server.incoming_requests() {
    thread_pool.execute(|| {
      if let Err(e) = handle_request(request) {
        // TODO: error
      }
    })
  }

  Ok(())
}

fn handle_request(request: Request) -> Result<(), IoError> {
  match request.url() {
    "/metrics" => request.respond(Response::from_string("metrics can be viewed here")),
    _ => request.respond(Response::from_string("404")),
  }
}

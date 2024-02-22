use std::{error::Error, io::Error as IoError, sync::Arc};

mod server;
use server::ThreadPool;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
  let server = Arc::new(tiny_http::Server::http("0.0.0.0:9630")?);
  let pool = ThreadPool::new(4);

  for request in server.incoming_requests() {
    pool.execute(|| {
      if let Err(e) = handle_request(request) {
        // TODO: error
      }
    })
  }

  Ok(())
}

fn handle_request(r: tiny_http::Request) -> Result<(), IoError> {
  match r.url() {
    "/metrics" => r.respond(tiny_http::Response::from_string(
      "metrics can be viewed here",
    )),
    _ => r.respond(tiny_http::Response::from_string("404")),
  }
}

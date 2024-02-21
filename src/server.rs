use std::{
  error::Error,
  io::Error as IoError,
  net::ToSocketAddrs,
  sync::Arc,
  thread::{self, JoinHandle},
};

pub struct ServerConfig<A> {
  pub address: A,
  pub workers: usize,
}

pub struct Server {
  server: Arc<tiny_http::Server>,
  handles: Vec<JoinHandle<()>>,
}

impl Server {
  pub fn new<A: ToSocketAddrs>(
    config: ServerConfig<A>,
  ) -> Result<Server, Box<dyn Error + Send + Sync>> {
    let server = Arc::new(tiny_http::Server::http(config.address)?);
    let handles = Vec::with_capacity(config.workers);
    Ok(Server { server, handles })
  }

  pub fn start(mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
    for _ in 0..self.handles.capacity() {
      let server = Arc::clone(&self.server);

      let handle = thread::spawn(move || loop {
        match server.try_recv() {
          Ok(value) => {
            if let Some(request) = value {
              if let Err(e) = handle_request(request) {
                eprintln!("Error: {:?}", e)
              }
            }
          }
          Err(e) => eprintln!("Error: {:?}", e),
        }
      });

      self.handles.push(handle);
    }

    for handle in self.handles {
      if let Err(e) = handle.join() {
        eprintln!("Error: {:?}", e);
      }
    }

    Ok(())
  }
}

fn handle_request(r: tiny_http::Request) -> Result<(), IoError> {
  match r.url() {
    "/metrics" => r.respond(tiny_http::Response::from_string(
      "metrics can be viewed here",
    )),
    _ => r.respond(tiny_http::Response::from_string("404")),
  }
}

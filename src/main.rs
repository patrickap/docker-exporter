use std::error::Error;

mod server;
use server::{Server, ServerConfig};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
  let server = Server::new(ServerConfig {
    address: String::from("0.0.0.0:9630"),
    workers: 4,
  });
  server?.start()
}

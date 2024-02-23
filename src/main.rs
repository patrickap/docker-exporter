use std::time::Duration;

use async_std::{io, net, prelude::*, task};

mod config;

#[async_std::main]
async fn main() -> io::Result<()> {
  let server = net::TcpListener::bind(config::SERVER_ADDRESS).await?;
  println!("server listening on {}", server.local_addr()?);

  while let Some(stream) = server.incoming().next().await {
    task::spawn(async move { handle_connection(stream?).await });
  }

  Ok(())
}

async fn handle_connection(mut stream: net::TcpStream) -> io::Result<()> {
  task::sleep(Duration::from_millis(5000)).await;

  let buffer_reader = io::BufReader::new(&stream);
  let request_line = buffer_reader.lines().next().await.unwrap()?;

  let (status, response) = if request_line == "GET / HTTP/1.1" {
    ("HTTP/1.1 200 OK", "hello world")
  } else {
    ("HTTP/1.1 404 NOT FOUND", "404")
  };

  let length = response.len();

  let response = format!("{status}\r\nContent-Length: {length}\r\n\r\n{response}");

  stream.write_all(response.as_bytes()).await?;

  Ok(())
}

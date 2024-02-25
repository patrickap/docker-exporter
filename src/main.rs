use actix_web as actix;
use std::{io, net};

#[actix::main]
async fn main() -> io::Result<()> {
  let address = net::SocketAddrV4::new(net::Ipv4Addr::new(0, 0, 0, 0), 9630);
  let server = actix::HttpServer::new(|| actix::App::new().service(metrics))
    .bind(address)?
    .workers(2)
    .run();

  println!("server listening on {:?}", address);
  server.await?;

  println!("server stopped");
  Ok(())
}

#[actix::get("/metrics")]
async fn metrics() -> Result<impl actix::Responder, actix::Error> {
  Ok(
    actix::HttpResponse::build(actix::http::StatusCode::OK)
      .content_type(actix::http::header::ContentType::plaintext())
      .body("Hello World"),
  )
}

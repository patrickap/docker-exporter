use actix_web;
use std;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let address = std::net::SocketAddrV4::new(std::net::Ipv4Addr::new(0, 0, 0, 0), 9630);
  let server = actix_web::HttpServer::new(|| actix_web::App::new().service(metrics))
    .bind(address)?
    .workers(2)
    .run();

  println!("server listening on {:?}", address);
  server.await?;

  println!("server stopped");
  Ok(())
}

#[actix_web::get("/metrics")]
async fn metrics() -> Result<impl actix_web::Responder, actix_web::Error> {
  Ok(
    actix_web::HttpResponse::build(actix_web::http::StatusCode::OK)
      .content_type(actix_web::http::header::ContentType::plaintext())
      .body("Hello World"),
  )
}

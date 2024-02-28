pub mod consts {
  pub const ADDRESS: &str = "0.0.0.0:9630";
}

pub mod routes {
  pub async fn status() -> &'static str {
    "ok"
  }

  pub async fn metrics() -> &'static str {
    "metrics"
  }
}

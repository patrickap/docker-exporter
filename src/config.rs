use std::time::Duration;

pub const SERVER_ADDRESS: &str = "0.0.0.0:9630";
pub const SERVER_WORKERS: usize = 4;
pub const SERVER_TIMEOUT: Duration = Duration::from_millis(1000);

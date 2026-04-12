use tracing_subscriber::fmt;

pub fn init_tracing(log_level: &str) {
    let filter = log_level.parse().unwrap_or(tracing::Level::INFO);

    fmt()
        .json()
        .with_max_level(filter)
        .with_target(true)
        .with_thread_ids(true)
        .init();
}

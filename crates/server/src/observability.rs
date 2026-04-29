use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;

pub fn init_tracing(log_level: &str) {
    // Prefer RUST_LOG env var; fall back to the configured log_level string.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    fmt()
        .json()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .init();
}

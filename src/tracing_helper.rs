use tracing_subscriber::prelude::*;

/// Init tracing
pub fn init_tracing() {
    let env = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
        .from_env_lossy();
    let fmt_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_file(true)
        .with_line_number(true);
    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(env)
        .init();
}
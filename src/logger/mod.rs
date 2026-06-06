use std::fs;

use chrono::Local;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use crate::{cli::LogLevel, error::Result};

pub fn init(level: LogLevel) -> Result<()> {
    let date = Local::now().format("%Y-%m-%d").to_string();
    let log_dir = std::path::PathBuf::from("logs").join(date);
    fs::create_dir_all(&log_dir)?;
    let file_appender = tracing_appender::rolling::never(log_dir, "httptest.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let filter = EnvFilter::new(match level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    });
    let _ = Box::leak(Box::new(_guard));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .try_init()
        .ok();
    Ok(())
}

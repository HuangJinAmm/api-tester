use std::{path::PathBuf, time::Duration};

use crate::cli::RunArgs;

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub connect_timeout: Duration,
    pub timeout: Duration,
    pub retry_count: usize,
    pub retry_delay: Duration,
    pub retry_backoff: bool,
    pub ca_cert: Option<PathBuf>,
    pub accept_invalid_certs: bool,
    pub cookie_file: Option<PathBuf>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(15),
            timeout: Duration::from_secs(60),
            retry_count: 0,
            retry_delay: Duration::from_millis(500),
            retry_backoff: false,
            ca_cert: None,
            accept_invalid_certs: true,
            cookie_file: None,
        }
    }
}

impl From<&RunArgs> for HttpConfig {
    fn from(args: &RunArgs) -> Self {
        Self {
            connect_timeout: Duration::from_secs(args.connect_timeout),
            timeout: Duration::from_secs(args.timeout),
            retry_count: args.retry_count,
            retry_delay: Duration::from_millis(args.retry_delay_ms),
            retry_backoff: args.retry_backoff,
            ca_cert: args.ca_cert.clone(),
            accept_invalid_certs: args.accept_invalid_certs,
            cookie_file: args.cookie_file.clone(),
        }
    }
}

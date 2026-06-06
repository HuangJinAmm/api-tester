use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "httptest", version, about = "Markdown-driven HTTP test CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, value_enum, default_value_t = LogLevel::Info, global = true)]
    pub log_level: LogLevel,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(name = "run", visible_alias = "r")]
    Run(RunArgs),
}

#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    pub target: PathBuf,

    #[arg(long)]
    pub load: bool,

    #[arg(long, default_value_t = 1)]
    pub users: usize,

    #[arg(long, default_value_t = 60)]
    pub duration: u64,

    #[arg(long)]
    pub qps: Option<u64>,

    #[arg(long, default_value_t = 0)]
    pub ramp_up: u64,

    #[arg(long)]
    pub max_requests: Option<u64>,

    #[arg(long, default_value_t = 15)]
    pub connect_timeout: u64,

    #[arg(long, default_value_t = 60)]
    pub timeout: u64,

    #[arg(long, default_value_t = 0)]
    pub retry_count: usize,

    #[arg(long, default_value_t = 500)]
    pub retry_delay_ms: u64,

    #[arg(long)]
    pub retry_backoff: bool,

    #[arg(long)]
    pub ca_cert: Option<PathBuf>,

    #[arg(long, default_value_t = true)]
    pub accept_invalid_certs: bool,

    #[arg(long)]
    pub cookie_file: Option<PathBuf>,

    #[arg(long)]
    pub report_json: Option<PathBuf>,

    #[arg(long)]
    pub report_junit: Option<PathBuf>,

    #[arg(long)]
    pub report_md: Option<PathBuf>,

    #[arg(long)]
    pub fail_fast: bool,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Cli {
    pub fn parse_args() -> Self {
        let args: Vec<_> = std::env::args_os().collect();
        if args
            .get(1)
            .is_some_and(|arg| !arg.to_string_lossy().starts_with('-'))
        {
            let mut normalized = vec![args[0].clone(), "run".into()];
            normalized.extend(args.into_iter().skip(1));
            Self::parse_from(normalized)
        } else {
            Self::parse()
        }
    }
}

use httptest::{
    cli::{Cli, Commands},
    error::Result,
    http::HttpConfig,
    runtime::Runtime,
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse_args();
    httptest::logger::init(cli.log_level)?;

    match cli.command {
        Commands::Run(args) => {
            let path = httptest::utils::resolve_case_path(&args.target);
            if httptest::utils::is_csv_path(&path) {
                httptest::csv_runner::run_csv(&path, &args).await?;
            } else if args.load {
                httptest::loadtest::run_load_test(&path, args).await?;
            } else {
                let mut runtime = Runtime::with_config(HttpConfig::from(&args))?;
                let result = runtime.run_case_path(&path).await?;
                if let Some(report_path) = &args.report_json {
                    httptest::report::write_case_json(&result, report_path)?;
                }
                if let Some(report_path) = &args.report_junit {
                    httptest::report::write_case_junit(&result, report_path)?;
                }
            }
        }
    }

    Ok(())
}

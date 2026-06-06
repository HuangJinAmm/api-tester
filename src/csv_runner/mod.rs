use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Local;
use serde::Deserialize;

use crate::{
    cli::RunArgs,
    error::{AppError, Result},
    http::HttpConfig,
    report::BatchReport,
    runtime::Runtime,
    utils,
};

#[derive(Debug, Deserialize)]
struct CsvCase {
    case: String,
    enabled: bool,
    repeat: Option<usize>,
    #[serde(flatten)]
    vars: HashMap<String, String>,
}

pub async fn run_csv(path: &Path, args: &RunArgs) -> Result<()> {
    let mut reader = csv::Reader::from_path(path)?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut runtime = Runtime::with_config(HttpConfig::from(args))?;
    let mut report = BatchReport::new();
    let mut batch_log = BatchLog::create()?;

    for row in reader.deserialize::<CsvCase>() {
        let row = row?;
        if !row.enabled {
            batch_log.write_line(&format!("SKIP {}", row.case))?;
            continue;
        }
        let case_name = row.case.clone();
        let case_path = utils::resolve_case_path(&base_dir.join(PathBuf::from(&row.case)));
        let vars = row
            .vars
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        for repeat in 0..row.repeat.unwrap_or(1) {
            match runtime.run_case_path_with_vars(&case_path, &vars).await {
                Ok(result) if result.status < 400 => {
                    batch_log.write_line(&format!(
                        "OK {} repeat={} status={} time_ms={}",
                        case_name,
                        repeat + 1,
                        result.status,
                        result.time_ms
                    ))?;
                    report.record_success(case_name.clone(), repeat + 1, &result);
                }
                Ok(result) => {
                    batch_log.write_line(&format!(
                        "ERR {} repeat={} status={} time_ms={}",
                        case_name,
                        repeat + 1,
                        result.status,
                        result.time_ms
                    ))?;
                    report.record_error(
                        case_name.clone(),
                        repeat + 1,
                        Some(result.status),
                        Some(result.time_ms),
                        None,
                    );
                    if args.fail_fast {
                        break;
                    }
                }
                Err(error) => {
                    batch_log.write_line(&format!(
                        "ERR {} repeat={} error={}",
                        case_name,
                        repeat + 1,
                        error
                    ))?;
                    report.record_error(
                        case_name.clone(),
                        repeat + 1,
                        None,
                        None,
                        Some(error.to_string()),
                    );
                    if args.fail_fast {
                        break;
                    }
                }
            }
        }
        if args.fail_fast && report.errors > 0 {
            break;
        }
    }

    println!("{}", report.summary());
    batch_log.write_line(&report.summary())?;
    if let Some(path) = &args.report_json {
        report.write_json(path)?;
    }
    if let Some(path) = &args.report_junit {
        report.write_junit(path)?;
    }
    if let Some(path) = &args.report_md {
        report.write_markdown(path)?;
    }
    if report.errors > 0 {
        return Err(AppError::Other(format!(
            "batch completed with {} error(s)",
            report.errors
        )));
    }
    Ok(())
}

struct BatchLog {
    file: std::fs::File,
}

impl BatchLog {
    fn create() -> Result<Self> {
        let now = Local::now();
        let log_dir = PathBuf::from("logs").join(now.format("%Y-%m-%d").to_string());
        std::fs::create_dir_all(&log_dir)?;
        let path = log_dir.join(format!("batch_{}.log", now.format("%Y%m%d%H%M%S")));
        Ok(Self {
            file: std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?,
        })
    }

    fn write_line(&mut self, line: &str) -> Result<()> {
        writeln!(self.file, "{} {line}", Local::now().to_rfc3339())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::CsvCase;

    #[test]
    fn parses_csv_row() {
        let mut reader = csv::Reader::from_reader(
            "case,enabled,repeat,username\nlogin,true,2,admin\n".as_bytes(),
        );
        let row = reader.deserialize::<CsvCase>().next().unwrap().unwrap();
        assert_eq!(row.case, "login");
        assert!(row.enabled);
        assert_eq!(row.repeat, Some(2));
        assert_eq!(row.vars["username"], "admin");
    }
}

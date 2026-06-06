use std::{path::Path, sync::Arc};

use chrono::Local;
use reqwest::cookie::Jar;
use tokio::io::AsyncWriteExt;

use crate::{
    assertion,
    error::Result,
    http::{HttpClient, HttpConfig},
    model::TestResult,
    parser, script,
    template::{TemplateContext, render},
};

pub struct Runtime {
    http: HttpClient,
    context: TemplateContext,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        Self::with_config(HttpConfig::default())
    }

    pub fn with_config(config: HttpConfig) -> Result<Self> {
        let cookie_store = Arc::new(Jar::default());
        Ok(Self {
            http: HttpClient::new(cookie_store, config)?,
            context: TemplateContext::with_env(),
        })
    }

    pub async fn run_case_path(&mut self, path: &Path) -> Result<TestResult> {
        self.run_case_path_with_vars(path, &[]).await
    }

    pub async fn run_case_path_with_vars(
        &mut self,
        path: &Path,
        vars: &[(&str, &str)],
    ) -> Result<TestResult> {
        let case = parser::parse_case_file(path)?;
        for (key, value) in vars {
            self.context.insert_string(*key, *value);
        }
        for var in &case.vars {
            self.context
                .insert_string(&var.name, render(&var.value, &self.context)?);
        }
        script::run_pre_script(&case, &mut self.context)?;
        let result = self.http.execute(&case, &self.context).await?;
        write_case_log(path, &case, &result).await?;
        assertion::verify(&case, &result, &self.context)?;
        script::run_post_script(&case, &result, &mut self.context)?;
        println!(
            "{} {} {}ms {}",
            case.name, result.status, result.time_ms, result.case_name
        );
        Ok(result)
    }
}

async fn write_case_log(
    path: &Path,
    case: &crate::model::TestCase,
    result: &TestResult,
) -> Result<()> {
    let date = Local::now().format("%Y-%m-%d").to_string();
    let file_name = path
        .file_stem()
        .map_or_else(|| "case".into(), |value| value.to_string_lossy());
    let log_dir = std::path::PathBuf::from("logs").join(date);
    tokio::fs::create_dir_all(&log_dir).await?;
    let log_path = log_dir.join(format!("{file_name}.log"));
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .await?;
    let body = match &case.body {
        Some(
            crate::model::Body::Json(value)
            | crate::model::Body::Text(value)
            | crate::model::Body::Raw(value)
            | crate::model::Body::FormUrlEncoded(value),
        ) => value.as_str(),
        None => "",
    };
    let entry = format!(
        "[{}]\ncase={}\nmethod={}\nurl={}\nheaders={:?}\nbody={}\nstatus={}\ntime_ms={}\nresponse_headers={:?}\nresponse_body={}\n\n",
        Local::now().to_rfc3339(),
        case.name,
        case.method,
        case.url,
        case.headers,
        body,
        result.status,
        result.time_ms,
        result.headers,
        result.body
    );
    file.write_all(entry.as_bytes()).await?;
    Ok(())
}

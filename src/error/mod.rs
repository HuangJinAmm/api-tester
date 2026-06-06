use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("parse error in {file:?}:{line}: {message}")]
    ParseError {
        file: Option<PathBuf>,
        line: usize,
        message: String,
    },
    #[error("http error for {url}: {source}")]
    HttpError {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("timeout error for {url}: {message}")]
    TimeoutError { url: String, message: String },
    #[error("script error in case {case}: {message}")]
    ScriptError { case: String, message: String },
    #[error("template error: {0}")]
    TemplateError(#[from] handlebars::RenderError),
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("csv error: {0}")]
    CsvError(#[from] csv::Error),
    #[error("assertion failed in case {case}: {message}")]
    AssertionError { case: String, message: String },
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("invalid header {key}: {message}")]
    HeaderError { key: String, message: String },
    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn parse(file: Option<PathBuf>, line: usize, message: impl Into<String>) -> Self {
        Self::ParseError {
            file,
            line,
            message: message.into(),
        }
    }
}

use std::{collections::HashMap, fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
    Trace,
    Connect,
}

impl HttpMethod {
    pub fn as_reqwest(&self) -> reqwest::Method {
        match self {
            Self::Get => reqwest::Method::GET,
            Self::Post => reqwest::Method::POST,
            Self::Put => reqwest::Method::PUT,
            Self::Delete => reqwest::Method::DELETE,
            Self::Patch => reqwest::Method::PATCH,
            Self::Head => reqwest::Method::HEAD,
            Self::Options => reqwest::Method::OPTIONS,
            Self::Trace => reqwest::Method::TRACE,
            Self::Connect => reqwest::Method::CONNECT,
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Trace => "TRACE",
            Self::Connect => "CONNECT",
        })
    }
}

impl FromStr for HttpMethod {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_uppercase().as_str() {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "DELETE" => Ok(Self::Delete),
            "PATCH" => Ok(Self::Patch),
            "HEAD" => Ok(Self::Head),
            "OPTIONS" => Ok(Self::Options),
            "TRACE" => Ok(Self::Trace),
            "CONNECT" => Ok(Self::Connect),
            _ => Err(AppError::parse(
                None,
                0,
                format!("unsupported method {value}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Json(String),
    Text(String),
    Raw(String),
    FormUrlEncoded(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadSpec {
    pub field: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarSpec {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestOptions {
    pub timeout_secs: Option<u64>,
    pub retry_count: Option<usize>,
    pub retry_delay_ms: Option<u64>,
    pub retry_backoff: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assertion {
    Status(u16),
    StatusIn { from: u16, to: u16 },
    HeaderEquals { key: String, value: String },
    HeaderNotEquals { key: String, value: String },
    HeaderExists { key: String },
    BodyContains(String),
    BodyNotContains(String),
    BodyMatches { regex: String },
    JsonEquals { path: String, value: String },
    JsonNotEquals { path: String, value: String },
    JsonExists { path: String },
    JsonType { path: String, ty: JsonType },
    LatencyMax { ms: u128 },
}

/// Expected JSON value type for `Assertion::JsonType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonType {
    String,
    Number,
    Bool,
    Array,
    Object,
    Null,
}

impl JsonType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Bool => "bool",
            Self::Array => "array",
            Self::Object => "object",
            Self::Null => "null",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "string" | "str" | "text" => Some(Self::String),
            "number" | "num" | "int" | "float" => Some(Self::Number),
            "bool" | "boolean" => Some(Self::Bool),
            "array" | "list" => Some(Self::Array),
            "object" | "obj" | "map" => Some(Self::Object),
            "null" | "none" => Some(Self::Null),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCase {
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Body>,
    pub pre_script: Option<String>,
    pub post_script: Option<String>,
    pub vars: Vec<VarSpec>,
    pub uploads: Vec<UploadSpec>,
    pub multipart_fields: Vec<FormField>,
    pub download: Option<String>,
    pub options: RequestOptions,
    pub assertions: Vec<Assertion>,
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub case_name: String,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub time_ms: u128,
}

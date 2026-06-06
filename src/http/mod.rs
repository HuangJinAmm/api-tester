mod config;

use std::{collections::HashMap, path::Path, sync::Arc, time::Instant};

pub use config::HttpConfig;
use futures::StreamExt;
use reqwest::{
    Client,
    cookie::{CookieStore, Jar},
    header::{HeaderMap, HeaderName, HeaderValue},
};
use tokio::{fs::File, io::AsyncWriteExt};
use tracing::{info, warn};
use url::Url;

use crate::{
    error::{AppError, Result},
    model::{Body, TestCase, TestResult},
    template::{TemplateContext, render},
};

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    cookie_store: Arc<Jar>,
    config: HttpConfig,
}

impl HttpClient {
    pub fn new(cookie_store: Arc<Jar>, config: HttpConfig) -> Result<Self> {
        load_cookies(&cookie_store, &config)?;
        let mut builder = Client::builder()
            .cookie_provider(Arc::clone(&cookie_store))
            .danger_accept_invalid_certs(config.accept_invalid_certs)
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout);

        if let Some(path) = &config.ca_cert {
            let cert = std::fs::read(path)?;
            let cert = reqwest::Certificate::from_pem(&cert).map_err(|source| {
                AppError::Other(format!("failed to load CA certificate {path:?}: {source}"))
            })?;
            builder = builder.add_root_certificate(cert);
        }

        let client = builder.build().map_err(|source| AppError::HttpError {
            url: "<client>".to_string(),
            source,
        })?;
        Ok(Self {
            client,
            cookie_store,
            config,
        })
    }

    pub async fn execute(&self, case: &TestCase, context: &TemplateContext) -> Result<TestResult> {
        let url = render(&case.url, context)?;
        let mut last_error = None;
        let retry_count = case.options.retry_count.unwrap_or(self.config.retry_count);

        for attempt in 0..=retry_count {
            match self.execute_once(case, context, &url).await {
                Ok(result) if should_retry_status(result.status) && attempt < retry_count => {
                    warn!(attempt, status = result.status, url = %url, "retrying request after status");
                    self.sleep_before_retry(case, attempt).await;
                }
                Ok(result) => {
                    persist_cookies(&self.cookie_store, &self.config, &url)?;
                    return Ok(result);
                }
                Err(error) if attempt < retry_count => {
                    warn!(attempt, error = %error, url = %url, "retrying request after error");
                    last_error = Some(error);
                    self.sleep_before_retry(case, attempt).await;
                }
                Err(error) => return Err(error),
            }
        }

        Err(last_error.unwrap_or_else(|| AppError::Other("request failed".to_string())))
    }

    async fn execute_once(
        &self,
        case: &TestCase,
        context: &TemplateContext,
        url: &str,
    ) -> Result<TestResult> {
        let request = self.build_request(case, context, url).await?;
        info!(method = %case.method, url = %url, "sending request");
        let started = Instant::now();
        let response = request.send().await.map_err(|source| AppError::HttpError {
            url: url.to_string(),
            source,
        })?;
        let status = response.status().as_u16();
        let response_headers = response
            .headers()
            .iter()
            .map(|(key, value)| {
                (
                    key.to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect::<HashMap<_, _>>();

        let body = if let Some(path) = &case.download {
            let bytes = stream_download(response, Path::new(&render(path, context)?), url).await?;
            format!("<downloaded {bytes} bytes>")
        } else {
            response
                .text()
                .await
                .map_err(|source| AppError::HttpError {
                    url: url.to_string(),
                    source,
                })?
        };
        let time_ms = started.elapsed().as_millis();

        if status >= 400 {
            warn!(status, url = %url, "request returned error status");
        }
        info!(status, time_ms, "request completed");

        Ok(TestResult {
            case_name: case.name.clone(),
            status,
            headers: response_headers,
            body,
            time_ms,
        })
    }

    async fn build_request(
        &self,
        case: &TestCase,
        context: &TemplateContext,
        url: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let headers = render_headers(&case.headers, context)?;
        let mut request = self
            .client
            .request(case.method.as_reqwest(), url)
            .headers(headers);
        if let Some(timeout) = case.options.timeout_secs {
            request = request.timeout(std::time::Duration::from_secs(timeout));
        }

        if !case.uploads.is_empty() || !case.multipart_fields.is_empty() {
            let mut form = reqwest::multipart::Form::new();
            for field in &case.multipart_fields {
                form = form.text(field.name.clone(), render(&field.value, context)?);
            }
            for upload in &case.uploads {
                let path = render(&upload.path, context)?;
                let part = reqwest::multipart::Part::file(&path)
                    .await
                    .map_err(AppError::IoError)?;
                form = form.part(upload.field.clone(), part);
            }
            request = request.multipart(form);
        } else if let Some(body) = &case.body {
            match body {
                Body::Json(value) => {
                    request = request
                        .header(reqwest::header::CONTENT_TYPE, "application/json")
                        .body(render(value, context)?);
                }
                Body::Text(value) => request = request.body(render(value, context)?),
                Body::Raw(value) => request = request.body(render(value, context)?),
                Body::FormUrlEncoded(value) => {
                    request = request
                        .header(
                            reqwest::header::CONTENT_TYPE,
                            "application/x-www-form-urlencoded",
                        )
                        .body(render(value, context)?);
                }
            }
        }

        Ok(request)
    }

    async fn sleep_before_retry(&self, case: &TestCase, attempt: usize) {
        let retry_backoff = case
            .options
            .retry_backoff
            .unwrap_or(self.config.retry_backoff);
        let retry_delay = case
            .options
            .retry_delay_ms
            .map(std::time::Duration::from_millis)
            .unwrap_or(self.config.retry_delay);
        let multiplier = if retry_backoff {
            2_u32.saturating_pow(attempt as u32)
        } else {
            1
        };
        tokio::time::sleep(retry_delay.saturating_mul(multiplier)).await;
    }
}

fn render_headers(
    headers: &HashMap<String, String>,
    context: &TemplateContext,
) -> Result<HeaderMap> {
    let mut map = HeaderMap::new();
    for (key, value) in headers {
        let name =
            HeaderName::from_bytes(key.as_bytes()).map_err(|error| AppError::HeaderError {
                key: key.clone(),
                message: error.to_string(),
            })?;
        let value = HeaderValue::from_str(&render(value, context)?).map_err(|error| {
            AppError::HeaderError {
                key: key.clone(),
                message: error.to_string(),
            }
        })?;
        map.insert(name, value);
    }
    Ok(map)
}

async fn stream_download(response: reqwest::Response, path: &Path, url: &str) -> Result<u64> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = File::create(path).await?;
    let mut stream = response.bytes_stream();
    let mut written = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|source| AppError::HttpError {
            url: url.to_string(),
            source,
        })?;
        written += chunk.len() as u64;
        file.write_all(&chunk).await?;
    }
    Ok(written)
}

fn should_retry_status(status: u16) -> bool {
    status == 408 || status == 429 || status >= 500
}

fn load_cookies(cookie_store: &Arc<Jar>, config: &HttpConfig) -> Result<()> {
    let Some(path) = &config.cookie_file else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(path)?;
    let cookies: Vec<PersistedCookie> = serde_json::from_str(&content)?;
    for cookie in cookies {
        let url = Url::parse(&cookie.url).map_err(|error| {
            AppError::Other(format!("invalid cookie url {}: {error}", cookie.url))
        })?;
        cookie_store.add_cookie_str(&cookie.cookie, &url);
    }
    Ok(())
}

fn persist_cookies(cookie_store: &Arc<Jar>, config: &HttpConfig, url: &str) -> Result<()> {
    let Some(path) = &config.cookie_file else {
        return Ok(());
    };
    let url =
        Url::parse(url).map_err(|error| AppError::Other(format!("invalid url {url}: {error}")))?;
    let Some(cookie_header) = cookie_store.cookies(&url) else {
        return Ok(());
    };
    let Some(cookie_header) = cookie_header.to_str().ok() else {
        return Ok(());
    };
    let cookies = vec![PersistedCookie {
        url: url.to_string(),
        cookie: cookie_header.to_string(),
    }];
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&cookies)?)?;
    Ok(())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PersistedCookie {
    url: String,
    cookie: String,
}

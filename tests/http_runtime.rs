use std::{collections::HashMap, fs};

use httptest::{
    http::{HttpClient, HttpConfig},
    model::{Body, FormField, HttpMethod, RequestOptions, TestCase},
    template::TemplateContext,
};
use reqwest::cookie::Jar;
use std::sync::Arc;
use tempfile::tempdir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string, body_string_contains, header, method, path},
};

fn case(method: HttpMethod, url: String) -> TestCase {
    TestCase {
        name: "case".to_string(),
        method,
        url,
        headers: HashMap::new(),
        body: None,
        pre_script: None,
        post_script: None,
        vars: Vec::new(),
        uploads: Vec::new(),
        multipart_fields: Vec::new(),
        download: None,
        options: RequestOptions::default(),
        assertions: Vec::new(),
    }
}

#[tokio::test]
async fn executes_http_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200).set_body_string("done"))
        .mount(&server)
        .await;

    let client = HttpClient::new(Arc::new(Jar::default()), HttpConfig::default()).unwrap();
    let result = client
        .execute(
            &case(HttpMethod::Get, format!("{}/ok", server.uri())),
            &TemplateContext::default(),
        )
        .await
        .unwrap();

    assert_eq!(result.status, 200);
    assert_eq!(result.body, "done");
}

#[tokio::test]
async fn retries_server_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let config = HttpConfig {
        retry_count: 1,
        retry_delay: std::time::Duration::from_millis(1),
        ..HttpConfig::default()
    };
    let client = HttpClient::new(Arc::new(Jar::default()), config).unwrap();
    let result = client
        .execute(
            &case(HttpMethod::Get, format!("{}/flaky", server.uri())),
            &TemplateContext::default(),
        )
        .await
        .unwrap();

    assert_eq!(result.status, 200);
}

#[tokio::test]
async fn streams_download_to_file() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/file"))
        .respond_with(ResponseTemplate::new(200).set_body_string("artifact"))
        .mount(&server)
        .await;
    let dir = tempdir().unwrap();
    let output = dir.path().join("artifact.txt");
    let mut test_case = case(HttpMethod::Get, format!("{}/file", server.uri()));
    test_case.download = Some(output.to_string_lossy().into_owned());

    let client = HttpClient::new(Arc::new(Jar::default()), HttpConfig::default()).unwrap();
    let result = client
        .execute(&test_case, &TemplateContext::default())
        .await
        .unwrap();

    assert_eq!(result.body, "<downloaded 8 bytes>");
    assert_eq!(fs::read_to_string(output).unwrap(), "artifact");
}

#[tokio::test]
async fn sends_json_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/json"))
        .respond_with(ResponseTemplate::new(201).set_body_string("created"))
        .mount(&server)
        .await;
    let mut test_case = case(HttpMethod::Post, format!("{}/json", server.uri()));
    test_case.body = Some(Body::Json(r#"{"name":"{{name}}"}"#.to_string()));
    let mut context = TemplateContext::default();
    context.insert_string("name", "alice");

    let client = HttpClient::new(Arc::new(Jar::default()), HttpConfig::default()).unwrap();
    let result = client.execute(&test_case, &context).await.unwrap();

    assert_eq!(result.status, 201);
}

#[tokio::test]
async fn sends_text_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/text"))
        .and(body_string("hello alice"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut test_case = case(HttpMethod::Post, format!("{}/text", server.uri()));
    test_case.body = Some(Body::Text("hello {{name}}".to_string()));
    let mut context = TemplateContext::default();
    context.insert_string("name", "alice");

    let client = HttpClient::new(Arc::new(Jar::default()), HttpConfig::default()).unwrap();
    let result = client.execute(&test_case, &context).await.unwrap();

    assert_eq!(result.status, 200);
}

#[tokio::test]
async fn sends_form_urlencoded_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/form"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("username=alice&password=secret"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut test_case = case(HttpMethod::Post, format!("{}/form", server.uri()));
    test_case.body = Some(Body::FormUrlEncoded(
        "username={{name}}&password={{password}}".to_string(),
    ));
    let mut context = TemplateContext::default();
    context.insert_string("name", "alice");
    context.insert_string("password", "secret");

    let client = HttpClient::new(Arc::new(Jar::default()), HttpConfig::default()).unwrap();
    let result = client.execute(&test_case, &context).await.unwrap();

    assert_eq!(result.status, 200);
}

#[tokio::test]
async fn sends_raw_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/raw"))
        .and(body_string("raw alice"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut test_case = case(HttpMethod::Post, format!("{}/raw", server.uri()));
    test_case.body = Some(Body::Raw("raw {{name}}".to_string()));
    let mut context = TemplateContext::default();
    context.insert_string("name", "alice");

    let client = HttpClient::new(Arc::new(Jar::default()), HttpConfig::default()).unwrap();
    let result = client.execute(&test_case, &context).await.unwrap();

    assert_eq!(result.status, 200);
}

#[tokio::test]
async fn sends_multipart_text_field() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/multipart"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut test_case = case(HttpMethod::Post, format!("{}/multipart", server.uri()));
    test_case.multipart_fields = vec![FormField {
        name: "description".to_string(),
        value: "hello {{name}}".to_string(),
    }];
    let mut context = TemplateContext::default();
    context.insert_string("name", "alice");

    let client = HttpClient::new(Arc::new(Jar::default()), HttpConfig::default()).unwrap();
    let result = client.execute(&test_case, &context).await.unwrap();

    assert_eq!(result.status, 200);
}

#[tokio::test]
async fn sends_multipart_file_upload() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/upload"))
        .and(body_string_contains("file-content"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let dir = tempdir().unwrap();
    let upload_path = dir.path().join("upload.txt");
    fs::write(&upload_path, "file-content").unwrap();
    let mut test_case = case(HttpMethod::Post, format!("{}/upload", server.uri()));
    test_case.uploads = vec![httptest::model::UploadSpec {
        field: "file".to_string(),
        path: upload_path.to_string_lossy().into_owned(),
    }];

    let client = HttpClient::new(Arc::new(Jar::default()), HttpConfig::default()).unwrap();
    let result = client
        .execute(&test_case, &TemplateContext::default())
        .await
        .unwrap();

    assert_eq!(result.status, 200);
}

#[tokio::test]
async fn applies_total_timeout() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("slow")
                .set_delay(std::time::Duration::from_millis(200)),
        )
        .mount(&server)
        .await;
    let config = HttpConfig {
        timeout: std::time::Duration::from_millis(20),
        ..HttpConfig::default()
    };
    let client = HttpClient::new(Arc::new(Jar::default()), config).unwrap();
    let error = client
        .execute(
            &case(HttpMethod::Get, format!("{}/slow", server.uri())),
            &TemplateContext::default(),
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("http error"));
}

#[tokio::test]
async fn persists_cookie_file() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/cookie"))
        .respond_with(ResponseTemplate::new(200).insert_header("set-cookie", "sid=abc; Path=/"))
        .mount(&server)
        .await;
    let dir = tempdir().unwrap();
    let cookie_file = dir.path().join("cookies.json");
    let config = HttpConfig {
        cookie_file: Some(cookie_file.clone()),
        ..HttpConfig::default()
    };

    let client = HttpClient::new(Arc::new(Jar::default()), config).unwrap();
    let result = client
        .execute(
            &case(HttpMethod::Get, format!("{}/cookie", server.uri())),
            &TemplateContext::default(),
        )
        .await
        .unwrap();

    assert_eq!(result.status, 200);
    assert!(fs::read_to_string(cookie_file).unwrap().contains("sid=abc"));
}

use std::{collections::HashMap, fs, path::Path};

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

use crate::{
    error::{AppError, Result},
    model::{
        Assertion, Body, FormField, HttpMethod, RequestOptions, TestCase, UploadSpec, VarSpec,
    },
};

pub fn parse_case_file(path: &Path) -> Result<TestCase> {
    let content = fs::read_to_string(path)?;
    parse_case(&content, Some(path))
}

pub fn parse_case(content: &str, path: Option<&Path>) -> Result<TestCase> {
    let parser = Parser::new(content);
    let mut in_h1 = false;
    let mut in_item = false;
    let mut in_code: Option<String> = None;
    let mut text = String::new();
    let mut code = String::new();

    let mut name = None;
    let mut method = None;
    let mut url = None;
    let mut body = None;
    let mut scripts = Vec::new();
    let mut state = ParseState::default();

    for event in parser {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            }) => {
                in_h1 = true;
                text.clear();
            }
            Event::End(TagEnd::Heading(HeadingLevel::H1)) => {
                let value = text.trim();
                if !value.is_empty() && name.is_none() {
                    name = Some(value.to_string());
                }
                in_h1 = false;
            }
            Event::Start(Tag::Item) => {
                in_item = true;
                text.clear();
            }
            Event::End(TagEnd::Item) => {
                state.parse_list_item(text.trim());
                in_item = false;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                in_code = Some(lang);
                code.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(lang) = in_code.take() {
                    match normalize_code_lang(&lang).as_str() {
                        "json" => body = Some(Body::Json(code.trim().to_string())),
                        "text" => body = Some(Body::Text(code.trim().to_string())),
                        "raw" => body = Some(Body::Raw(code.trim().to_string())),
                        "form" => body = Some(Body::FormUrlEncoded(code.trim().to_string())),
                        "rhai" => scripts.push(code.trim().to_string()),
                        "py" | "python" => scripts.push(code.trim().to_string()),
                        _ => {}
                    }
                }
            }
            Event::Text(value) | Event::Code(value) => {
                if in_code.is_some() {
                    code.push_str(&value);
                } else if in_h1 || in_item {
                    text.push_str(&value);
                } else if method.is_none() {
                    let value = value.trim();
                    if let Some((parsed_method, parsed_url)) = parse_request_line(value) {
                        method = Some(parsed_method);
                        url = Some(parsed_url);
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak if in_code.is_some() => {
                code.push('\n');
            }
            _ => {}
        }
    }

    let case = TestCase {
        name: name.unwrap_or_else(|| {
            path.and_then(Path::file_stem)
                .map_or("unnamed".into(), |s| s.to_string_lossy().into_owned())
        }),
        method: method.ok_or_else(|| {
            AppError::parse(path.map(Path::to_path_buf), 0, "missing request line")
        })?,
        url: url.ok_or_else(|| {
            AppError::parse(path.map(Path::to_path_buf), 0, "missing request url")
        })?,
        headers: state.headers,
        body,
        pre_script: scripts.first().cloned(),
        post_script: scripts.get(1).cloned(),
        vars: state.vars,
        uploads: state.uploads,
        multipart_fields: state.multipart_fields,
        download: state.download,
        options: state.options,
        assertions: state.assertions,
    };

    Ok(case)
}

fn parse_request_line(value: &str) -> Option<(HttpMethod, String)> {
    let mut parts = value.split_whitespace();
    let method = parts.next()?.parse().ok()?;
    let url = parts.next()?.to_string();
    if parts.next().is_some() {
        return None;
    }
    Some((method, url))
}

fn normalize_code_lang(lang: &str) -> String {
    match lang.trim().to_ascii_lowercase().as_str() {
        "form" | "urlencoded" | "form-urlencoded" | "x-www-form-urlencoded" => "form".to_string(),
        value => value.to_string(),
    }
}

#[derive(Default)]
struct ParseState {
    headers: HashMap<String, String>,
    vars: Vec<VarSpec>,
    uploads: Vec<UploadSpec>,
    multipart_fields: Vec<FormField>,
    download: Option<String>,
    options: RequestOptions,
    assertions: Vec<Assertion>,
}

impl ParseState {
    fn parse_list_item(&mut self, value: &str) {
        let Some((key, raw_value)) = value.split_once(':') else {
            return;
        };
        let key = key.trim();
        let raw_value = raw_value.trim();

        if key.eq_ignore_ascii_case("upload") {
            if let Some((field, path)) = raw_value.split_once('=') {
                self.uploads.push(UploadSpec {
                    field: field.trim().to_string(),
                    path: path.trim().to_string(),
                });
            }
        } else if key.eq_ignore_ascii_case("field") {
            if let Some((name, value)) = raw_value.split_once('=') {
                self.multipart_fields.push(FormField {
                    name: name.trim().to_string(),
                    value: value.trim().to_string(),
                });
            }
        } else if key.eq_ignore_ascii_case("var") {
            if let Some((name, value)) = raw_value.split_once('=') {
                self.vars.push(VarSpec {
                    name: name.trim().to_string(),
                    value: value.trim().to_string(),
                });
            }
        } else if key.eq_ignore_ascii_case("download") {
            self.download = Some(raw_value.to_string());
        } else if key.eq_ignore_ascii_case("timeout") {
            self.options.timeout_secs = raw_value.parse().ok();
        } else if key.eq_ignore_ascii_case("retry-count") {
            self.options.retry_count = raw_value.parse().ok();
        } else if key.eq_ignore_ascii_case("retry-delay-ms") {
            self.options.retry_delay_ms = raw_value.parse().ok();
        } else if key.eq_ignore_ascii_case("retry-backoff") {
            self.options.retry_backoff = parse_bool(raw_value);
        } else if key.eq_ignore_ascii_case("assert-status") {
            if let Ok(status) = raw_value.parse() {
                self.assertions.push(Assertion::Status(status));
            }
        } else if key.eq_ignore_ascii_case("assert-header") {
            if let Some((header, value)) = raw_value.split_once('=') {
                self.assertions.push(Assertion::HeaderEquals {
                    key: header.trim().to_ascii_lowercase(),
                    value: value.trim().to_string(),
                });
            }
        } else if key.eq_ignore_ascii_case("assert-body-contains") {
            self.assertions
                .push(Assertion::BodyContains(raw_value.to_string()));
        } else if key.eq_ignore_ascii_case("assert-json") {
            if let Some((path, value)) = raw_value.split_once('=') {
                self.assertions.push(Assertion::JsonEquals {
                    path: path.trim().to_string(),
                    value: value.trim().to_string(),
                });
            }
        } else if !key.is_empty() {
            self.headers.insert(key.to_string(), raw_value.to_string());
        }
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" => Some(true),
        "false" | "0" | "no" | "n" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markdown_case() {
        let md = r#"
# Login

ignored text

POST https://example.com/login

- Content-Type:application/json
- token:{{token}}
- timeout:10
- retry-count:2
- var:token=abc
- field:description=hello {{ts}}
- upload:avatar=./avatar.png
- assert-status:200
- assert-json:user.name=alice

```rhai
vars["ts"] = now()
```

```json
{"time":"{{ts}}"}
```

```rhai
if response.status != 200 { fail("bad"); }
```
"#;
        let case = parse_case(md, None).unwrap();
        assert_eq!(case.name, "Login");
        assert_eq!(case.method, HttpMethod::Post);
        assert_eq!(case.url, "https://example.com/login");
        assert_eq!(case.headers["Content-Type"], "application/json");
        assert_eq!(case.options.timeout_secs, Some(10));
        assert_eq!(case.options.retry_count, Some(2));
        assert_eq!(case.vars[0].name, "token");
        assert_eq!(case.multipart_fields[0].name, "description");
        assert_eq!(case.uploads[0].field, "avatar");
        assert_eq!(case.assertions.len(), 2);
        assert!(case.pre_script.unwrap().contains("now"));
        assert!(case.post_script.unwrap().contains("response.status"));
    }

    #[test]
    fn parses_non_json_body_blocks() {
        let text = parse_case(
            "# Text\nPOST https://example.test\n```text\nhello\n```",
            None,
        )
        .unwrap();
        assert_eq!(text.body, Some(Body::Text("hello".to_string())));

        let raw = parse_case("# Raw\nPOST https://example.test\n```raw\nabc\n```", None).unwrap();
        assert_eq!(raw.body, Some(Body::Raw("abc".to_string())));

        let form = parse_case(
            "# Form\nPOST https://example.test\n```form-urlencoded\na=1&b={{b}}\n```",
            None,
        )
        .unwrap();
        assert_eq!(
            form.body,
            Some(Body::FormUrlEncoded("a=1&b={{b}}".to_string()))
        );
    }
}

use serde_json::Value;

use crate::{
    error::{AppError, Result},
    model::{Assertion, TestCase, TestResult},
    template::{TemplateContext, render},
};

pub fn verify(case: &TestCase, result: &TestResult, context: &TemplateContext) -> Result<()> {
    for assertion in &case.assertions {
        match assertion {
            Assertion::Status(expected) => {
                if result.status != *expected {
                    return Err(failed(
                        case,
                        format!("expected status {expected}, got {}", result.status),
                    ));
                }
            }
            Assertion::HeaderEquals { key, value } => {
                let expected = render(value, context)?;
                let actual = result
                    .headers
                    .get(&key.to_ascii_lowercase())
                    .or_else(|| result.headers.get(key))
                    .map(String::as_str);
                if actual != Some(expected.as_str()) {
                    return Err(failed(
                        case,
                        format!("expected header {key}={expected}, got {actual:?}"),
                    ));
                }
            }
            Assertion::BodyContains(value) => {
                let expected = render(value, context)?;
                if !result.body.contains(&expected) {
                    return Err(failed(
                        case,
                        format!("expected body to contain {expected:?}"),
                    ));
                }
            }
            Assertion::JsonEquals { path, value } => {
                let json: Value = serde_json::from_str(&result.body)?;
                let actual = lookup_json_path(&json, path).ok_or_else(|| {
                    failed(
                        case,
                        format!("json path {path} was not present in response"),
                    )
                })?;
                let expected = render(value, context)?;
                if !json_value_matches(actual, &expected) {
                    return Err(failed(
                        case,
                        format!("expected json {path}={expected}, got {actual}"),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn failed(case: &TestCase, message: String) -> AppError {
    AppError::AssertionError {
        case: case.name.clone(),
        message,
    }
}

fn lookup_json_path<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = json;
    for part in path.trim_start_matches("$.").split('.') {
        if part.is_empty() {
            continue;
        }
        current = current.get(part)?;
    }
    Some(current)
}

fn json_value_matches(actual: &Value, expected: &str) -> bool {
    match actual {
        Value::String(value) => value == expected,
        Value::Number(value) => value.to_string() == expected,
        Value::Bool(value) => value.to_string() == expected,
        Value::Null => expected.eq_ignore_ascii_case("null"),
        Value::Array(_) | Value::Object(_) => serde_json::from_str::<Value>(expected)
            .is_ok_and(|expected_json| actual == &expected_json),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::model::{HttpMethod, RequestOptions};

    fn case(assertions: Vec<Assertion>) -> TestCase {
        TestCase {
            name: "assert".to_string(),
            method: HttpMethod::Get,
            url: "http://example.test".to_string(),
            headers: HashMap::new(),
            body: None,
            pre_script: None,
            post_script: None,
            vars: Vec::new(),
            uploads: Vec::new(),
            multipart_fields: Vec::new(),
            download: None,
            options: RequestOptions::default(),
            assertions,
        }
    }

    fn result() -> TestResult {
        TestResult {
            case_name: "assert".to_string(),
            status: 200,
            headers: HashMap::from([("content-type".to_string(), "application/json".to_string())]),
            body: r#"{"ok":true,"user":{"name":"alice"}}"#.to_string(),
            time_ms: 10,
        }
    }

    #[test]
    fn verifies_declared_assertions() {
        verify(
            &case(vec![
                Assertion::Status(200),
                Assertion::HeaderEquals {
                    key: "content-type".to_string(),
                    value: "application/json".to_string(),
                },
                Assertion::BodyContains("alice".to_string()),
                Assertion::JsonEquals {
                    path: "user.name".to_string(),
                    value: "alice".to_string(),
                },
            ]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap();
    }
}

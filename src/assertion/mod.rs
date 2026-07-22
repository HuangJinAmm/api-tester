use serde_json::Value;

use crate::{
    error::{AppError, Result},
    model::{Assertion, JsonType, TestCase, TestResult},
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
            Assertion::StatusIn { from, to } => {
                if result.status < *from || result.status > *to {
                    return Err(failed(
                        case,
                        format!(
                            "expected status in range {from}-{to}, got {}",
                            result.status
                        ),
                    ));
                }
            }
            Assertion::HeaderEquals { key, value } => {
                let expected = render(value, context)?;
                let actual = lookup_header(result, key);
                if actual != Some(expected.as_str()) {
                    return Err(failed(
                        case,
                        format!("expected header {key}={expected}, got {actual:?}"),
                    ));
                }
            }
            Assertion::HeaderNotEquals { key, value } => {
                let expected = render(value, context)?;
                let actual = lookup_header(result, key);
                if actual == Some(expected.as_str()) {
                    return Err(failed(
                        case,
                        format!("expected header {key} != {expected}, but matched"),
                    ));
                }
            }
            Assertion::HeaderExists { key } => {
                if lookup_header(result, key).is_none() {
                    return Err(failed(case, format!("expected header {key} to be present")));
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
            Assertion::BodyNotContains(value) => {
                let expected = render(value, context)?;
                if result.body.contains(&expected) {
                    return Err(failed(
                        case,
                        format!("expected body to NOT contain {expected:?}"),
                    ));
                }
            }
            Assertion::BodyMatches { regex } => {
                let pattern = render(regex, context)?;
                let re = regex::Regex::new(&pattern)
                    .map_err(|e| failed(case, format!("invalid regex {pattern:?}: {e}")))?;
                if !re.is_match(&result.body) {
                    return Err(failed(
                        case,
                        format!("expected body to match regex {pattern:?}"),
                    ));
                }
            }
            Assertion::JsonEquals { path, value } => {
                let json = parse_json_body(case, &result.body)?;
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
            Assertion::JsonNotEquals { path, value } => {
                let json = parse_json_body(case, &result.body)?;
                let expected = render(value, context)?;
                match lookup_json_path(&json, path) {
                    Some(actual) if json_value_matches(actual, &expected) => Err(failed(
                        case,
                        format!("expected json {path} != {expected}, but matched"),
                    )),
                    None => Err(failed(
                        case,
                        format!("json path {path} was not present in response"),
                    )),
                    _ => Ok(()),
                }?;
            }
            Assertion::JsonExists { path } => {
                let json = parse_json_body(case, &result.body)?;
                if lookup_json_path(&json, path).is_none() {
                    return Err(failed(case, format!("expected json path {path} to exist")));
                }
            }
            Assertion::JsonType { path, ty } => {
                let json = parse_json_body(case, &result.body)?;
                let actual = lookup_json_path(&json, path).ok_or_else(|| {
                    failed(
                        case,
                        format!("json path {path} was not present in response"),
                    )
                })?;
                if !json_type_matches(actual, *ty) {
                    return Err(failed(
                        case,
                        format!(
                            "expected json {path} to be {}, got {}",
                            ty.as_str(),
                            json_type_name(actual)
                        ),
                    ));
                }
            }
            Assertion::LatencyMax { ms } => {
                if result.time_ms > *ms {
                    return Err(failed(
                        case,
                        format!("expected latency <= {ms}ms, got {}ms", result.time_ms),
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

fn lookup_header<'a>(result: &'a TestResult, key: &str) -> Option<&'a str> {
    result
        .headers
        .get(&key.to_ascii_lowercase())
        .or_else(|| result.headers.get(key))
        .map(String::as_str)
}

fn parse_json_body(case: &TestCase, body: &str) -> Result<Value> {
    serde_json::from_str(body)
        .map_err(|e| failed(case, format!("response body is not valid json: {e}")))
}

fn lookup_json_path<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = json;
    for part in path.trim_start_matches("$.").split('.') {
        if part.is_empty() {
            continue;
        }
        current = resolve_path_segment(current, part)?;
    }
    Some(current)
}

/// Resolve a single path segment that may include array indices or wildcard.
/// Supports forms: `user`, `users[0]`, `users[*]`, `users[0].name`.
fn resolve_path_segment<'a>(json: &'a Value, segment: &str) -> Option<&'a Value> {
    // Split leading key (no brackets) from any bracketed indices.
    let (key, rest) = match segment.find('[') {
        Some(idx) => (&segment[..idx], &segment[idx..]),
        None => (segment, ""),
    };

    let mut current = if key.is_empty() { json } else { json.get(key)? };

    // Parse bracket groups like `[0]`, `[*]`, `[2]` (potentially chained).
    let mut chars = rest.chars().peekable();
    while chars.peek() == Some(&'[') {
        chars.next(); // consume '['
        let mut inner = String::new();
        while let Some(&c) = chars.peek() {
            if c == ']' {
                chars.next();
                break;
            }
            inner.push(c);
            chars.next();
        }
        let trimmed = inner.trim();
        if trimmed == "*" {
            // For wildcard, return the first element if it's an array (best-effort
            // existence check); null/empty arrays resolve to None.
            return current.as_array().and_then(|arr| arr.first());
        } else if let Ok(idx) = trimmed.parse::<usize>() {
            current = current.get(idx)?;
        } else {
            return None;
        }
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

fn json_type_matches(actual: &Value, ty: JsonType) -> bool {
    matches!(
        (actual, ty),
        (Value::String(_), JsonType::String)
            | (Value::Number(_), JsonType::Number)
            | (Value::Bool(_), JsonType::Bool)
            | (Value::Array(_), JsonType::Array)
            | (Value::Object(_), JsonType::Object)
            | (Value::Null, JsonType::Null)
    )
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "bool",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
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
            body: r#"{"ok":true,"user":{"name":"alice","age":30},"tags":["a","b","c"],"items":[{"id":1},{"id":2}]}"#.to_string(),
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

    #[test]
    fn verifies_status_in_range() {
        verify(
            &case(vec![Assertion::StatusIn { from: 200, to: 299 }]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap();
    }

    #[test]
    fn fails_when_status_out_of_range() {
        let err = verify(
            &case(vec![Assertion::StatusIn { from: 400, to: 599 }]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("400-599"));
    }

    #[test]
    fn verifies_header_exists_and_not_equals() {
        verify(
            &case(vec![
                Assertion::HeaderExists {
                    key: "content-type".to_string(),
                },
                Assertion::HeaderNotEquals {
                    key: "content-type".to_string(),
                    value: "text/html".to_string(),
                },
            ]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap();
    }

    #[test]
    fn fails_when_header_missing() {
        let err = verify(
            &case(vec![Assertion::HeaderExists {
                key: "x-trace".to_string(),
            }]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("x-trace"));
    }

    #[test]
    fn verifies_body_not_contains() {
        verify(
            &case(vec![Assertion::BodyNotContains("error".to_string())]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap();
    }

    #[test]
    fn verifies_body_matches_regex() {
        verify(
            &case(vec![Assertion::BodyMatches {
                regex: r#""name":"alice""#.to_string(),
            }]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap();
    }

    #[test]
    fn fails_on_invalid_regex() {
        let err = verify(
            &case(vec![Assertion::BodyMatches {
                regex: r"[".to_string(),
            }]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("invalid regex"));
    }

    #[test]
    fn verifies_json_exists_and_not_equals() {
        verify(
            &case(vec![
                Assertion::JsonExists {
                    path: "user.name".to_string(),
                },
                Assertion::JsonNotEquals {
                    path: "user.name".to_string(),
                    value: "admin".to_string(),
                },
            ]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap();
    }

    #[test]
    fn fails_when_json_path_missing_for_not_equals() {
        let err = verify(
            &case(vec![Assertion::JsonNotEquals {
                path: "user.missing".to_string(),
                value: "x".to_string(),
            }]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("not present"));
    }

    #[test]
    fn verifies_json_type() {
        verify(
            &case(vec![
                Assertion::JsonType {
                    path: "user.name".to_string(),
                    ty: JsonType::String,
                },
                Assertion::JsonType {
                    path: "user.age".to_string(),
                    ty: JsonType::Number,
                },
                Assertion::JsonType {
                    path: "ok".to_string(),
                    ty: JsonType::Bool,
                },
                Assertion::JsonType {
                    path: "tags".to_string(),
                    ty: JsonType::Array,
                },
                Assertion::JsonType {
                    path: "user".to_string(),
                    ty: JsonType::Object,
                },
            ]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap();
    }

    #[test]
    fn resolves_array_index_in_json_path() {
        verify(
            &case(vec![
                Assertion::JsonEquals {
                    path: "tags[0]".to_string(),
                    value: "a".to_string(),
                },
                Assertion::JsonEquals {
                    path: "items[1].id".to_string(),
                    value: "2".to_string(),
                },
            ]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap();
    }

    #[test]
    fn resolves_array_wildcard_existence() {
        verify(
            &case(vec![Assertion::JsonExists {
                path: "tags[*]".to_string(),
            }]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap();
    }

    #[test]
    fn fails_when_array_index_out_of_bounds() {
        let err = verify(
            &case(vec![Assertion::JsonExists {
                path: "tags[99]".to_string(),
            }]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("tags[99]"));
        assert!(err.to_string().contains("exist"));
    }

    #[test]
    fn verifies_latency_max() {
        verify(
            &case(vec![Assertion::LatencyMax { ms: 50 }]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap();
    }

    #[test]
    fn fails_when_latency_exceeds_max() {
        let err = verify(
            &case(vec![Assertion::LatencyMax { ms: 5 }]),
            &result(),
            &TemplateContext::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("latency"));
    }

    #[test]
    fn json_type_parse_accepts_aliases() {
        assert_eq!(JsonType::parse("string"), Some(JsonType::String));
        assert_eq!(JsonType::parse("STR"), Some(JsonType::String));
        assert_eq!(JsonType::parse("int"), Some(JsonType::Number));
        assert_eq!(JsonType::parse("boolean"), Some(JsonType::Bool));
        assert_eq!(JsonType::parse("list"), Some(JsonType::Array));
        assert_eq!(JsonType::parse("nonsense"), None);
    }
}

use httptest::{parser, template};

#[test]
fn parses_and_renders_case() {
    let md = r#"
# Case
GET https://example.com/{{path}}
- Authorization:Bearer {{token}}
```json
{"ok":true}
```
"#;
    let case = parser::parse_case(md, None).unwrap();
    let mut context = template::TemplateContext::default();
    context.insert_string("path", "health");
    context.insert_string("token", "secret");
    assert_eq!(
        template::render(&case.url, &context).unwrap(),
        "https://example.com/health"
    );
    assert_eq!(
        template::render(&case.headers["Authorization"], &context).unwrap(),
        "Bearer secret"
    );
}

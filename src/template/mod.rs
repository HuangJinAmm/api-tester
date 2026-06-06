use std::collections::HashMap;

use handlebars::Handlebars;
use serde_json::{Map, Value};

use crate::error::Result;

#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    values: Map<String, Value>,
}

impl TemplateContext {
    pub fn with_env() -> Self {
        let mut values = Map::new();
        for (key, value) in std::env::vars() {
            values.insert(format!("env_{key}"), Value::String(value));
        }
        Self { values }
    }

    pub fn insert_string(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into(), Value::String(value.into()));
    }

    pub fn merge_csv(&mut self, row: &HashMap<String, String>) {
        for (key, value) in row {
            self.insert_string(key, value);
        }
    }

    pub fn as_json(&self) -> Value {
        Value::Object(self.values.clone())
    }
}

pub fn render(value: &str, context: &TemplateContext) -> Result<String> {
    let handlebars = Handlebars::new();
    Ok(handlebars.render_template(value, &context.as_json())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_vars() {
        let mut ctx = TemplateContext::default();
        ctx.insert_string("token", "abc");
        assert_eq!(render("Bearer {{token}}", &ctx).unwrap(), "Bearer abc");
    }
}

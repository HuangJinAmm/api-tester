use std::collections::HashMap;

use rhai::{Dynamic, Engine, Map, Scope};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    model::{TestCase, TestResult},
    template::TemplateContext,
};

pub fn run_pre_script(case: &TestCase, context: &mut TemplateContext) -> Result<()> {
    if let Some(script) = &case.pre_script {
        let vars = run_script(case, script, None)?;
        for (key, value) in vars {
            context.insert_string(key, value.to_string());
        }
    }
    Ok(())
}

pub fn run_post_script(
    case: &TestCase,
    result: &TestResult,
    context: &mut TemplateContext,
) -> Result<()> {
    if let Some(script) = &case.post_script {
        let vars = run_script(case, script, Some(result))?;
        for (key, value) in vars {
            context.insert_string(key, value.to_string());
        }
    }
    Ok(())
}

fn run_script(
    case: &TestCase,
    script: &str,
    response: Option<&TestResult>,
) -> Result<HashMap<String, Dynamic>> {
    let mut engine = Engine::new();
    engine.register_fn("now", || chrono::Utc::now().to_rfc3339());
    engine.register_fn("uuid", || Uuid::new_v4().to_string());
    engine.register_fn("log", |message: &str| tracing::info!("{message}"));
    engine.register_fn("sleep", |millis: i64| {
        std::thread::sleep(std::time::Duration::from_millis(millis as u64))
    });
    engine.register_fn(
        "fail",
        |message: &str| -> std::result::Result<(), Box<rhai::EvalAltResult>> {
            Err(message.into())
        },
    );

    let mut scope = Scope::new();
    scope.push("vars", Map::new());
    scope.push("env", env_map());
    scope.push("request", request_map(case));
    scope.push("response", response.map_or_else(Map::new, response_map));

    let script = script.replace("response.json()", "response.json");
    let _ = engine
        .eval_with_scope::<Dynamic>(&mut scope, &script)
        .map_err(|error| AppError::ScriptError {
            case: case.name.clone(),
            message: error.to_string(),
        })?;

    let vars = scope
        .get_value::<Map>("vars")
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect();
    Ok(vars)
}

fn env_map() -> Map {
    std::env::vars()
        .map(|(key, value)| (key.into(), Dynamic::from(value)))
        .collect()
}

fn request_map(case: &TestCase) -> Map {
    let mut map = Map::new();
    map.insert("name".into(), case.name.clone().into());
    map.insert("method".into(), case.method.to_string().into());
    map.insert("url".into(), case.url.clone().into());
    map
}

fn response_map(result: &TestResult) -> Map {
    let mut map = Map::new();
    map.insert("status".into(), (result.status as i64).into());
    map.insert("body".into(), result.body.clone().into());
    map.insert("time_ms".into(), (result.time_ms as i64).into());
    let headers = result
        .headers
        .iter()
        .map(|(key, value)| (key.clone().into(), Dynamic::from(value.clone())))
        .collect();
    map.insert("headers".into(), Dynamic::from_map(headers));
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&result.body) {
        map.insert("json".into(), json_to_dynamic(json));
    }
    map
}

fn json_to_dynamic(value: serde_json::Value) -> Dynamic {
    match value {
        serde_json::Value::Null => ().into(),
        serde_json::Value::Bool(value) => value.into(),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                value.into()
            } else if let Some(value) = value.as_f64() {
                value.into()
            } else {
                value.to_string().into()
            }
        }
        serde_json::Value::String(value) => value.into(),
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(json_to_dynamic)
            .collect::<rhai::Array>()
            .into(),
        serde_json::Value::Object(values) => values
            .into_iter()
            .map(|(key, value)| (key.into(), json_to_dynamic(value)))
            .collect::<Map>()
            .into(),
    }
}

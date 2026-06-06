# httptest

`httptest` is a Rust CLI for running HTTP tests from Markdown files. It targets Windows first and also supports Linux and macOS.

## Install

```bash
cargo build --release
```

Run a case:

```bash
cargo run -- examples/login.md
```

The binary accepts `run` explicitly or infers it:

```bash
httptest login_test
httptest run ./cases/user/login.md
httptest batch.csv
```

Request controls:

```bash
httptest examples/login.md --timeout 30 --connect-timeout 5
httptest examples/login.md --retry-count 3 --retry-delay-ms 200 --retry-backoff
httptest examples/login.md --ca-cert ./certs/root.pem --accept-invalid-certs false
httptest examples/login.md --cookie-file ./.httptest/cookies.json
```

Per-case controls can be placed in the Markdown header list:

```md
- timeout:30
- retry-count:3
- retry-delay-ms:200
- retry-backoff:true
- var:token=abc
- field:description=hello {{username}}
- upload:avatar=./avatar.png
- assert-status:200
- assert-header:content-type=application/json
- assert-body-contains:token
- assert-json:user.name=admin
```

## Markdown DSL

Only these Markdown elements are parsed:

- H1 title as the test name
- Request line: `METHOD URL`
- Header list items: `- key:value`
- One body code block: `json`, `text`, `raw`, `form`, `form-urlencoded`, or `x-www-form-urlencoded`
- First `rhai` code block as pre-script
- Second `rhai` code block as post-script

Example:

````md
# Login

POST https://api.test.com/login

- Content-Type:application/json
- token:{{token}}

```rhai
vars["ts"] = now()
```

```json
{
  "username": "admin",
  "password": "123456",
  "time": "{{ts}}"
}
```

```rhai
if response.status != 200 { fail("login failed"); }
```
````

Scripts use `rhai` fences and Rhai syntax. Legacy `py` fences are still accepted for older cases.

Body examples:

````md
```text
plain text body
```

```raw
raw bytes represented as text
```

```form
username={{username}}&password={{password}}
```
````

## Variables

Templates use Handlebars syntax:

```text
{{token}}
```

Supported sources in the current implementation:

- environment variables as `{{env_NAME}}`
- pre-script `vars`
- CSV row values

## CSV Batch

```csv
case,enabled,repeat
login,true,10
user/create,true,5
order/query,false,1
```

Extra CSV columns are injected as template variables for that row:

```csv
case,enabled,repeat,username,password
login.md,true,1,admin,123456
```

Run:

```bash
httptest batch.csv
httptest batch.csv --report-json reports/batch.json
httptest batch.csv --report-md reports/batch.md
httptest batch.csv --fail-fast
```

Batch mode prints a summary and writes JSON, JUnit XML, or Markdown reports when requested. It attempts all enabled rows and returns an error if any case failed.

## Load Test

```bash
httptest examples/login.md --load --users 100 --duration 60
httptest examples/login.md --load --users 20 --qps 50 --report-json reports/load.json
httptest examples/login.md --report-junit reports/login.xml
httptest batch.csv --report-junit reports/batch.xml
```

Supported flags:

- `--users`
- `--duration`
- `--ramp-up`
- `--max-requests`
- `--qps`
- `--report-json`
- `--report-junit`
- `--report-md`

Without `--load`, `--report-json` writes a single-case JSON report. `--report-junit` writes JUnit XML for single-case, batch, and load runs.

Output includes total requests, QPS, TPS, average latency, P90, P95, P99, success rate, and error count.

## Logs

Logs are written under:

```text
logs/YYYY-MM-DD/httptest.log
logs/YYYY-MM-DD/<case>.log
```

Log level:

```bash
httptest --log-level debug examples/login.md
```

## Development

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

## Roadmap

The current implementation focuses on the first MVP stages: Markdown parsing, CLI, HTTP request execution, JSON/text/raw/form-urlencoded bodies, headers, logging, templates, request-level variables, Rhai scripts, CSV batch variables and summary reports, cookie-backed sessions with optional persistence, downloads, multipart file/text fields, request timeout controls, custom CA loading, retry/backoff, fixed-QPS load throttling, JSON load reports, and async load-test metrics.

Planned extensions include richer cookie jar import/export, response assertions as first-class DSL, fixed-QPS scheduler precision improvements, streaming download progress, OpenAPI import, WebSocket/gRPC/MQTT, YAML DSL, plugin APIs, and Web UI.

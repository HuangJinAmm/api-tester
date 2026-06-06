---
name: httptest
description: Use the local Rust httptest CLI to create, run, debug, and report Markdown-defined HTTP API tests. Trigger when the user asks to test HTTP APIs, write .md API test cases, run batch.csv scenarios, execute load tests, validate responses with assertions, use CSV-driven variables, inspect request/response logs, or generate JSON reports with this repository's httptest tool.
---

# httptest

Use this skill when working in the `httptest` repository or when the user asks to use its CLI for API testing.

## Workflow

1. Check the repo builds before relying on the tool:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

2. Create or edit Markdown cases under `examples/` or a user-specified cases directory.

3. Run a single case:

```bash
cargo run -- examples/login.md
```

4. Run a CSV batch:

```bash
cargo run -- examples/batch.csv
```

5. Run a load test:

```bash
cargo run -- examples/loadtest.md --load --users 20 --duration 60 --qps 50
```

6. Use JSON reports when the user needs machine-readable output:

```bash
cargo run -- examples/login.md --report-json reports/login.json
cargo run -- examples/loadtest.md --load --report-json reports/load.json
cargo run -- examples/batch.csv --report-junit reports/batch.xml
cargo run -- examples/batch.csv --report-md reports/batch.md
```

## Markdown Case Format

Use this structure:

````md
# Case Name

POST https://api.example.com/login

- Content-Type:application/json
- Authorization:Bearer {{token}}
- timeout:30
- retry-count:3
- retry-delay-ms:200
- retry-backoff:true
- var:token=abc
- field:description=hello {{username}}
- assert-status:200
- assert-header:content-type=application/json
- assert-body-contains:token
- assert-json:user.name=admin

```rhai
vars["trace_id"] = uuid()
```

```json
{
  "username": "{{username}}",
  "trace": "{{trace_id}}"
}
```

```rhai
if response.status != 200 { fail("request failed"); }
```
````

Rules:

- Use one H1 for the test name.
- Use one request line: `METHOD URL`.
- Use `- key:value` for headers and controls.
- Use the first `rhai` block as the pre-script and the second `rhai` block as the post-script.
- Use `json`, `text`, `raw`, `form`, `form-urlencoded`, or `x-www-form-urlencoded` for request body blocks.
- Template variables use Handlebars syntax: `{{name}}`.

## Supported Controls

Per case:

```md
- timeout:30
- retry-count:3
- retry-delay-ms:200
- retry-backoff:true
- download:./output/file.bin
- upload:file=./input.bin
- upload:avatar=./avatar.png
- field:description=hello {{username}}
```

CLI-level:

```bash
--timeout 60
--connect-timeout 15
--retry-count 3
--retry-delay-ms 500
--retry-backoff
--ca-cert ./certs/root.pem
--accept-invalid-certs false
--cookie-file .httptest/cookies.json
```

## Assertions

Prefer declaration assertions for simple checks:

```md
- assert-status:200
- assert-header:content-type=application/json
- assert-body-contains:success
- assert-json:data.id=123
```

Use post-script Rhai only for more complex logic:

```rhai
if response.status != 200 { fail("bad status"); }
if response.json.token == "" { fail("missing token"); }
```

## CSV Batch

Use required columns plus any extra variable columns:

```csv
case,enabled,repeat,username,password
login.md,true,1,admin,123456
loadtest.md,true,5,,
```

Extra columns are available as `{{username}}`, `{{password}}`, etc. Cases are resolved relative to the CSV file.

Use `--report-json reports/batch.json` to write a batch summary report. Batch mode attempts all enabled rows and returns an error if any execution failed.
Use `--report-junit reports/batch.xml` for CI test report ingestion.
Use `--report-md reports/batch.md` for a human-readable Markdown summary with overview, failures, slowest executions, and all executions.
Use `--fail-fast` when CI should stop after the first failed execution.

## Load Testing

Use async load testing flags:

```bash
cargo run -- examples/loadtest.md --load --users 100 --duration 60 --ramp-up 10 --qps 200 --max-requests 10000
```

Metrics include total, success, errors, QPS, TPS, average latency, P90, P95, P99, success rate, and error rate.

## Logs And Outputs

Generated files:

- `logs/YYYY-MM-DD/httptest.log`
- `logs/YYYY-MM-DD/<case>.log`
- `reports/*.json` when `--report-json` is used
- downloaded files under the path specified by `download`
- cookie persistence under `--cookie-file`

Do not commit generated logs, reports, cookies, binaries, or downloaded output.

## Validation Before Finishing

After editing cases or source code, run:

```bash
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build
```

If a command fails because dependencies cannot be downloaded, rerun the needed Cargo command with network approval.

## Current Tool Limits

- Legacy `py` script fences are still accepted, but new cases should use `rhai`.
- Cookie persistence is basic JSON persistence, not a full browser cookie jar format.
- Fixed QPS scheduling is simple global throttling.
- Upload supports multipart file fields, but not ordinary multipart text fields yet.
- Parser error line numbers are not precise for all Markdown constructs.

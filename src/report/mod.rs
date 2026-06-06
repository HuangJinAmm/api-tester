use std::path::Path;

use chrono::Local;
use serde::Serialize;

use crate::error::Result;
use crate::model::TestResult;

#[derive(Debug, Default, Clone)]
pub struct LoadReport {
    latencies: Vec<u128>,
    success: u64,
    errors: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BatchReport {
    pub generated_at: String,
    pub total: u64,
    pub success: u64,
    pub errors: u64,
    pub records: Vec<BatchRecord>,
}

impl BatchReport {
    pub fn new() -> Self {
        Self {
            generated_at: Local::now().to_rfc3339(),
            ..Self::default()
        }
    }

    pub fn record_success(&mut self, case: String, repeat: usize, result: &TestResult) {
        self.total += 1;
        self.success += 1;
        self.records.push(BatchRecord {
            case,
            repeat,
            ok: true,
            status: Some(result.status),
            time_ms: Some(result.time_ms),
            error: None,
        });
    }

    pub fn record_error(
        &mut self,
        case: String,
        repeat: usize,
        status: Option<u16>,
        time_ms: Option<u128>,
        error: Option<String>,
    ) {
        self.total += 1;
        self.errors += 1;
        self.records.push(BatchRecord {
            case,
            repeat,
            ok: false,
            status,
            time_ms,
            error,
        });
    }

    pub fn summary(&self) -> String {
        let success_rate = if self.total == 0 {
            0.0
        } else {
            self.success as f64 * 100.0 / self.total as f64
        };
        format!(
            "batch total={} success={} errors={} success_rate={success_rate:.2}%",
            self.total, self.success, self.errors
        )
    }

    pub fn write_json(&self, path: &Path) -> Result<()> {
        write_parent_dir(path)?;
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn write_junit(&self, path: &Path) -> Result<()> {
        write_parent_dir(path)?;
        let mut testcases = String::new();
        for record in &self.records {
            let name = format!("{}#{}", record.case, record.repeat);
            let time = record.time_ms.unwrap_or_default() as f64 / 1000.0;
            if record.ok {
                testcases.push_str(&format!(
                    "  <testcase classname=\"httptest.batch\" name=\"{}\" time=\"{time:.3}\" />\n",
                    xml_escape(&name)
                ));
            } else {
                let message = record
                    .error
                    .clone()
                    .unwrap_or_else(|| format!("HTTP status {:?}", record.status));
                testcases.push_str(&format!(
                    "  <testcase classname=\"httptest.batch\" name=\"{}\" time=\"{time:.3}\"><failure message=\"{}\">{}</failure></testcase>\n",
                    xml_escape(&name),
                    xml_escape(&message),
                    xml_escape(&message)
                ));
            }
        }
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"httptest.batch\" tests=\"{}\" failures=\"{}\" errors=\"0\">\n{testcases}</testsuite>\n",
            self.total, self.errors
        );
        std::fs::write(path, xml)?;
        Ok(())
    }

    pub fn write_markdown(&self, path: &Path) -> Result<()> {
        write_parent_dir(path)?;
        std::fs::write(path, self.to_markdown())?;
        Ok(())
    }

    pub fn to_markdown(&self) -> String {
        let success_rate = if self.total == 0 {
            0.0
        } else {
            self.success as f64 * 100.0 / self.total as f64
        };
        let avg_ms = average_time_ms(&self.records);
        let mut output = String::new();
        output.push_str("# httptest Batch Summary\n\n");
        output.push_str("## Overview\n\n");
        output.push_str("| Metric | Value |\n");
        output.push_str("| --- | ---: |\n");
        output.push_str(&format!(
            "| Generated At | {} |\n",
            md_escape(&self.generated_at)
        ));
        output.push_str(&format!("| Total | {} |\n", self.total));
        output.push_str(&format!("| Success | {} |\n", self.success));
        output.push_str(&format!("| Errors | {} |\n", self.errors));
        output.push_str(&format!("| Success Rate | {success_rate:.2}% |\n"));
        output.push_str(&format!("| Average Time | {avg_ms:.2} ms |\n\n"));

        output.push_str("## Failures\n\n");
        let failures = self.records.iter().filter(|record| !record.ok);
        if failures.clone().next().is_none() {
            output.push_str("No failures.\n\n");
        } else {
            output.push_str("| Case | Repeat | Status | Time | Error |\n");
            output.push_str("| --- | ---: | ---: | ---: | --- |\n");
            for record in failures {
                output.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    md_escape(&record.case),
                    record.repeat,
                    optional_u16(record.status),
                    optional_time(record.time_ms),
                    md_escape(record.error.as_deref().unwrap_or(""))
                ));
            }
            output.push('\n');
        }

        output.push_str("## Slowest Executions\n\n");
        let mut slowest = self
            .records
            .iter()
            .filter(|record| record.time_ms.is_some())
            .collect::<Vec<_>>();
        slowest.sort_by_key(|record| std::cmp::Reverse(record.time_ms.unwrap_or_default()));
        if slowest.is_empty() {
            output.push_str("No timing data.\n\n");
        } else {
            output.push_str("| Case | Repeat | Result | Status | Time |\n");
            output.push_str("| --- | ---: | --- | ---: | ---: |\n");
            for record in slowest.into_iter().take(10) {
                output.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    md_escape(&record.case),
                    record.repeat,
                    if record.ok { "OK" } else { "ERR" },
                    optional_u16(record.status),
                    optional_time(record.time_ms)
                ));
            }
            output.push('\n');
        }

        output.push_str("## All Executions\n\n");
        if self.records.is_empty() {
            output.push_str("No executions.\n");
        } else {
            output.push_str("| Case | Repeat | Result | Status | Time | Error |\n");
            output.push_str("| --- | ---: | --- | ---: | ---: | --- |\n");
            for record in &self.records {
                output.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} |\n",
                    md_escape(&record.case),
                    record.repeat,
                    if record.ok { "OK" } else { "ERR" },
                    optional_u16(record.status),
                    optional_time(record.time_ms),
                    md_escape(record.error.as_deref().unwrap_or(""))
                ));
            }
        }
        output
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchRecord {
    pub case: String,
    pub repeat: usize,
    pub ok: bool,
    pub status: Option<u16>,
    pub time_ms: Option<u128>,
    pub error: Option<String>,
}

pub fn write_case_json(result: &TestResult, path: &Path) -> Result<()> {
    write_parent_dir(path)?;
    std::fs::write(
        path,
        serde_json::to_string_pretty(&CaseReport::from(result))?,
    )?;
    Ok(())
}

pub fn write_case_junit(result: &TestResult, path: &Path) -> Result<()> {
    write_parent_dir(path)?;
    let failure = if result.status >= 400 {
        format!(
            "<failure message=\"HTTP {}\">{}</failure>",
            result.status,
            xml_escape(&result.body)
        )
    } else {
        String::new()
    };
    let failures = u8::from(result.status >= 400);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"httptest\" tests=\"1\" failures=\"{failures}\" errors=\"0\" time=\"{:.3}\">\n  <testcase classname=\"httptest\" name=\"{}\" time=\"{:.3}\">{failure}</testcase>\n</testsuite>\n",
        result.time_ms as f64 / 1000.0,
        xml_escape(&result.case_name),
        result.time_ms as f64 / 1000.0
    );
    std::fs::write(path, xml)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct CaseReport<'a> {
    case_name: &'a str,
    status: u16,
    headers: &'a std::collections::HashMap<String, String>,
    body: &'a str,
    time_ms: u128,
}

impl<'a> From<&'a TestResult> for CaseReport<'a> {
    fn from(result: &'a TestResult) -> Self {
        Self {
            case_name: &result.case_name,
            status: result.status,
            headers: &result.headers,
            body: &result.body,
            time_ms: result.time_ms,
        }
    }
}

impl LoadReport {
    pub fn record_success(&mut self, latency: u128) {
        self.success += 1;
        self.latencies.push(latency);
    }

    pub fn record_error(&mut self, latency: u128) {
        self.errors += 1;
        self.latencies.push(latency);
    }

    pub fn summary(&self, duration_secs: u64) -> String {
        let metrics = self.metrics(duration_secs);
        format!(
            "total={} success={} errors={} qps={:.2} tps={:.2} avg_ms={:.2} p90={} p95={} p99={} success_rate={:.2}%",
            metrics.total,
            metrics.success,
            metrics.errors,
            metrics.qps,
            metrics.tps,
            metrics.avg_ms,
            metrics.p90_ms,
            metrics.p95_ms,
            metrics.p99_ms,
            metrics.success_rate
        )
    }

    pub fn metrics(&self, duration_secs: u64) -> LoadMetrics {
        let total = self.success + self.errors;
        let mut latencies = self.latencies.clone();
        latencies.sort_unstable();
        let avg = if latencies.is_empty() {
            0.0
        } else {
            latencies.iter().sum::<u128>() as f64 / latencies.len() as f64
        };
        let duration = duration_secs.max(1) as f64;
        let success_rate = if total == 0 {
            0.0
        } else {
            self.success as f64 * 100.0 / total as f64
        };
        LoadMetrics {
            generated_at: Local::now().to_rfc3339(),
            duration_secs,
            total,
            success: self.success,
            errors: self.errors,
            qps: total as f64 / duration,
            tps: self.success as f64 / duration,
            avg_ms: avg,
            p90_ms: percentile(&latencies, 90.0),
            p95_ms: percentile(&latencies, 95.0),
            p99_ms: percentile(&latencies, 99.0),
            success_rate,
            error_rate: 100.0 - success_rate,
        }
    }

    pub fn write_json(&self, duration_secs: u64, path: &Path) -> Result<()> {
        write_parent_dir(path)?;
        std::fs::write(
            path,
            serde_json::to_string_pretty(&self.metrics(duration_secs))?,
        )?;
        Ok(())
    }

    pub fn write_junit(&self, duration_secs: u64, path: &Path) -> Result<()> {
        write_parent_dir(path)?;
        let metrics = self.metrics(duration_secs);
        let message = format!(
            "total={} success={} errors={} qps={:.2} avg_ms={:.2} p95={}",
            metrics.total,
            metrics.success,
            metrics.errors,
            metrics.qps,
            metrics.avg_ms,
            metrics.p95_ms
        );
        let failure = if metrics.errors > 0 {
            format!(
                "<failure message=\"{}\">{}</failure>",
                xml_escape(&message),
                xml_escape(&message)
            )
        } else {
            String::new()
        };
        let failures = u8::from(metrics.errors > 0);
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"httptest.load\" tests=\"1\" failures=\"{failures}\" errors=\"0\" time=\"{duration_secs}\">\n  <testcase classname=\"httptest.load\" name=\"load\" time=\"{duration_secs}\">{failure}</testcase>\n</testsuite>\n"
        );
        std::fs::write(path, xml)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadMetrics {
    pub generated_at: String,
    pub duration_secs: u64,
    pub total: u64,
    pub success: u64,
    pub errors: u64,
    pub qps: f64,
    pub tps: f64,
    pub avg_ms: f64,
    pub p90_ms: u128,
    pub p95_ms: u128,
    pub p99_ms: u128,
    pub success_rate: f64,
    pub error_rate: f64,
}

fn percentile(values: &[u128], pct: f64) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let idx = ((pct / 100.0) * (values.len().saturating_sub(1)) as f64).round() as usize;
    values[idx]
}

fn write_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn md_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ")
}

fn optional_u16(value: Option<u16>) -> String {
    value.map_or_else(|| "-".to_string(), |value| value.to_string())
}

fn optional_time(value: Option<u128>) -> String {
    value.map_or_else(|| "-".to_string(), |value| format!("{value} ms"))
}

fn average_time_ms(records: &[BatchRecord]) -> f64 {
    let times = records
        .iter()
        .filter_map(|record| record.time_ms)
        .collect::<Vec<_>>();
    if times.is_empty() {
        0.0
    } else {
        times.iter().sum::<u128>() as f64 / times.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_summary() {
        let mut report = LoadReport::default();
        report.record_success(10);
        report.record_error(30);
        let summary = report.summary(1);
        assert!(summary.contains("total=2"));
        assert!(summary.contains("success_rate=50.00%"));
    }

    #[test]
    fn computes_batch_summary() {
        let mut report = BatchReport::new();
        let result = TestResult {
            case_name: "login".to_string(),
            status: 200,
            headers: std::collections::HashMap::new(),
            body: String::new(),
            time_ms: 12,
        };
        report.record_success("login.md".to_string(), 1, &result);
        report.record_error(
            "broken.md".to_string(),
            1,
            None,
            None,
            Some("failed".to_string()),
        );
        assert_eq!(report.total, 2);
        assert_eq!(report.success, 1);
        assert_eq!(report.errors, 1);
        assert!(report.summary().contains("success_rate=50.00%"));
    }

    #[test]
    fn writes_batch_junit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("batch.xml");
        let mut report = BatchReport::new();
        report.record_error(
            "broken&case.md".to_string(),
            1,
            Some(500),
            Some(10),
            Some("bad <status>".to_string()),
        );
        report.write_junit(&path).unwrap();
        let xml = std::fs::read_to_string(path).unwrap();
        assert!(xml.contains("<testsuite name=\"httptest.batch\""));
        assert!(xml.contains("failures=\"1\""));
        assert!(xml.contains("broken&amp;case.md#1"));
        assert!(xml.contains("bad &lt;status&gt;"));
    }

    #[test]
    fn writes_batch_markdown_summary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("batch.md");
        let mut report = BatchReport::new();
        let result = TestResult {
            case_name: "ok".to_string(),
            status: 200,
            headers: std::collections::HashMap::new(),
            body: String::new(),
            time_ms: 42,
        };
        report.record_success("ok.md".to_string(), 1, &result);
        report.record_error(
            "bad|case.md".to_string(),
            2,
            Some(500),
            Some(75),
            Some("line1\nline2".to_string()),
        );
        report.write_markdown(&path).unwrap();
        let markdown = std::fs::read_to_string(path).unwrap();
        assert!(markdown.contains("# httptest Batch Summary"));
        assert!(markdown.contains("| Total | 2 |"));
        assert!(markdown.contains("| Errors | 1 |"));
        assert!(markdown.contains("bad\\|case.md"));
        assert!(markdown.contains("line1 line2"));
        assert!(markdown.contains("## Slowest Executions"));
        assert!(markdown.contains("75 ms"));
    }

    #[test]
    fn writes_case_junit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("case.xml");
        let result = TestResult {
            case_name: "case".to_string(),
            status: 200,
            headers: std::collections::HashMap::new(),
            body: String::new(),
            time_ms: 25,
        };
        write_case_junit(&result, &path).unwrap();
        let xml = std::fs::read_to_string(path).unwrap();
        assert!(xml.contains("<testsuite name=\"httptest\""));
        assert!(xml.contains("failures=\"0\""));
    }
}

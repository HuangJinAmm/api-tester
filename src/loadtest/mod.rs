use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::future::join_all;
use tokio::sync::Mutex;

use crate::{cli::RunArgs, error::Result, http::HttpConfig, report::LoadReport, runtime::Runtime};

pub async fn run_load_test(path: &Path, args: RunArgs) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(args.duration);
    let max_requests = args.max_requests.unwrap_or(u64::MAX);
    let report = Arc::new(Mutex::new(LoadReport::default()));
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let path = Arc::new(path.to_path_buf());
    let qps_limiter = args.qps.map(|qps| {
        Arc::new(Mutex::new(QpsLimiter::new(Duration::from_secs_f64(
            1.0 / qps.max(1) as f64,
        ))))
    });

    let tasks = (0..args.users.max(1)).map(|user_idx| {
        let report = Arc::clone(&report);
        let counter = Arc::clone(&counter);
        let path = Arc::clone(&path);
        let ramp_up = args.ramp_up;
        let config = HttpConfig::from(&args);
        let qps_limiter = qps_limiter.clone();
        tokio::spawn(async move {
            if ramp_up > 0 {
                let delay = ramp_up.saturating_mul(user_idx as u64) / args.users.max(1) as u64;
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
            let mut runtime = Runtime::with_config(config)?;
            while Instant::now() < deadline {
                let request_no = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if request_no >= max_requests {
                    break;
                }
                if let Some(limiter) = &qps_limiter {
                    limiter.lock().await.wait_turn().await;
                }
                if Instant::now() >= deadline {
                    break;
                }
                let started = Instant::now();
                let result = runtime.run_case_path(&path).await;
                let elapsed = started.elapsed().as_millis();
                let mut report = report.lock().await;
                match result {
                    Ok(result) if result.status < 400 => report.record_success(result.time_ms),
                    Ok(result) => report.record_error(result.time_ms),
                    Err(_) => report.record_error(elapsed),
                }
            }
            crate::error::Result::<()>::Ok(())
        })
    });

    for task in join_all(tasks).await {
        task.map_err(|error| crate::error::AppError::Other(error.to_string()))??;
    }

    let report = report.lock().await;
    println!("{}", report.summary(args.duration));
    if let Some(path) = &args.report_json {
        report.write_json(args.duration, path)?;
    }
    if let Some(path) = &args.report_junit {
        report.write_junit(args.duration, path)?;
    }
    Ok(())
}

struct QpsLimiter {
    interval: Duration,
    next: Instant,
}

impl QpsLimiter {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            next: Instant::now(),
        }
    }

    async fn wait_turn(&mut self) {
        let now = Instant::now();
        if self.next > now {
            tokio::time::sleep(self.next - now).await;
        }
        self.next = Instant::now() + self.interval;
    }
}

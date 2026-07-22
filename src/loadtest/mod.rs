use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::future::join_all;
use tokio::sync::Semaphore;

use crate::{cli::RunArgs, error::Result, http::HttpConfig, report::LoadReport, runtime::Runtime};

pub async fn run_load_test(path: &Path, args: RunArgs) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(args.duration);
    let max_requests = args.max_requests.unwrap_or(u64::MAX);
    let report = Arc::new(tokio::sync::Mutex::new(LoadReport::default()));
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let path = Arc::new(path.to_path_buf());
    // Token-bucket limiter: producer refills one token per 1/qps interval using
    // a tokio interval with `MissedTickBehavior::Delay`, which avoids the
    // drift the old `next = Instant::now() + interval` accumulated. Capacity
    // equals the user count so concurrent users can be in-flight together
    // while the refill rate still caps the aggregate QPS at `args.qps`.
    let qps_limiter = args.qps.map(|qps| QpsLimiter::new(qps, args.users.max(1)));

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
                    limiter.acquire().await;
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

/// Token-bucket QPS limiter.
///
/// A dedicated refiller task wakes every `1/qps` seconds and adds one permit
/// to a `Semaphore`, up to `capacity`. Consumers acquire a permit (and forget
/// it) to send a request. Compared to the previous `Mutex<QpsLimiter>` design:
/// - No mutex: multiple users acquire concurrently instead of serializing on
///   `wait_turn`.
/// - No drift: `tokio::time::interval` with `MissedTickBehavior::Delay` keeps
///   the refill cadence anchored to the start time, instead of
///   `Instant::now() + interval` which drifted under load.
/// - Bounded burst: capacity caps how many tokens can accumulate when the
///   producer outruns the consumers, preventing catch-up bursts.
pub struct QpsLimiter {
    sem: Arc<Semaphore>,
}

impl QpsLimiter {
    pub fn new(qps: u64, capacity: usize) -> Self {
        let sem = Arc::new(Semaphore::new(0));
        let refill = Arc::clone(&sem);
        let interval = Duration::from_secs_f64(1.0 / qps.max(1) as f64);
        let capacity = capacity.max(1);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // Delay instead of Burst/Skip: a missed tick extends the next
            // deadline rather than firing immediately, keeping the long-term
            // rate anchored to `interval` and avoiding burst catch-up.
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // tokio::time::interval fires its first tick immediately; consume
            // it so the first real token arrives after one interval (matches
            // the previous "first wait blocks until next slot" behavior).
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if refill.available_permits() < capacity {
                    refill.add_permits(1);
                }
            }
        });
        Self { sem }
    }

    pub async fn acquire(&self) {
        // forget(): the permit represents a token consumed from the bucket,
        // not a lock to release. Returning it would let the bucket grow
        // without bound.
        if let Ok(permit) = self.sem.acquire().await {
            permit.forget();
        }
    }
}

impl Clone for QpsLimiter {
    fn clone(&self) -> Self {
        Self {
            sem: Arc::clone(&self.sem),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Loose smoke test: at qps=100 over 400ms we should issue roughly 40
    /// acquires (±40% to absorb tokio scheduling jitter). The old drift-prone
    /// implementation undercounted on tight loops because each `Instant::now()
    /// + interval` deferred the next slot.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn qps_limiter_refills_at_target_rate() {
        let limiter = QpsLimiter::new(100, 4);
        let start = Instant::now();
        let mut count = 0usize;
        while start.elapsed() < Duration::from_millis(400) {
            limiter.acquire().await;
            count += 1;
        }
        // 100 qps * 0.4s = 40 expected; allow 24..56 for jitter.
        assert!(
            (24..=56).contains(&count),
            "expected ~40 acquires in 400ms at qps=100, got {count}"
        );
    }

    /// Capacity bound prevents the bucket from growing past `capacity` when
    /// the producer outpaces consumers (i.e. it bounds the burst size).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn qps_limiter_caps_available_permits_at_capacity() {
        // High qps, tiny capacity, idle consumer: producer should saturate
        // at `capacity` and never exceed it.
        let limiter = QpsLimiter::new(10_000, 3);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let available = limiter.sem.available_permits();
        assert!(
            available <= 3,
            "expected <= 3 available permits (capacity), got {available}"
        );
    }
}

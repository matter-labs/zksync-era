// src/metrics.rs
//! Per-second TPS / latency reporter (no balance snapshot).

use hdrhistogram::Histogram;
use parking_lot::{Mutex, RwLock};
use std::{
    collections::VecDeque,
    io::Write,
    sync::{atomic::{AtomicU64, Ordering}, Arc},
    time::{Duration, Instant},
};
use tokio::time::interval;

/// Returns the median (p50) of a sorted slice
fn median(vals: &[u64]) -> u64 {
    if vals.is_empty() { 0 } else { vals[vals.len()/2] }
}

#[derive(Clone)]
pub struct Metrics {
    pub sent:     Arc<AtomicU64>,
    pub included: Arc<AtomicU64>,
    pub submit:   Arc<RwLock<Histogram<u64>>>,
    pub include:  Arc<RwLock<Histogram<u64>>>,
    pub sub_last: Arc<Mutex<VecDeque<(Instant, u64)>>>,
    pub inc_last: Arc<Mutex<VecDeque<(Instant, u64)>>>,
}

impl Metrics {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            sent:     Arc::new(AtomicU64::new(0)),
            included: Arc::new(AtomicU64::new(0)),
            submit:   Arc::new(RwLock::new(Histogram::new_with_max(60_000, 3)?)),
            include:  Arc::new(RwLock::new(Histogram::new_with_max(60_000, 3)?)),
            sub_last: Arc::new(Mutex::new(VecDeque::new())),
            inc_last: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    pub fn spawn_reporter(&self, started: Instant) {
        let me = self.clone();
        tokio::spawn(async move { me.report_loop(started).await });
    }

    async fn report_loop(self, started: Instant) {
        let mut tick = interval(Duration::from_secs(1));
        let mut tps_q: VecDeque<(Instant, u64)> = VecDeque::new();
        let mut last_inc = 0;

        loop {
            tick.tick().await;
            let now = Instant::now();

            // ── TPS window ──
            while tps_q.front().map_or(false, |(t, _)| *t + Duration::from_secs(10) < now) {
                tps_q.pop_front();
            }
            let inc_now  = self.included.load(Ordering::Relaxed);
            let delta_inc = inc_now - last_inc;
            last_inc = inc_now;
            tps_q.push_back((now, delta_inc));
            let tps10: u64 = tps_q.iter().map(|(_, d)| *d).sum();
            let tps_avg = inc_now as f64 / started.elapsed().as_secs_f64();

            // ── latency ──
            let sub_p50_tot = self.submit.read().value_at_quantile(0.5);
            let inc_p50_tot = self.include.read().value_at_quantile(0.5);
            let sub_p50_10 = {
                let mut dq = self.sub_last.lock();
                dq.retain(|(t, _)| *t + Duration::from_secs(10) >= now);
                let mut v: Vec<u64> = dq.iter().map(|(_, x)| *x).collect();
                v.sort(); median(&v)
            };
            let inc_p50_10 = {
                let mut dq = self.inc_last.lock();
                dq.retain(|(t, _)| *t + Duration::from_secs(10) >= now);
                let mut v: Vec<u64> = dq.iter().map(|(_, x)| *x).collect();
                v.sort(); median(&v)
            };

            let in_flight = self.sent.load(Ordering::Relaxed).saturating_sub(inc_now);
            println!(
                "⏱ {:>4}s | sent {:>7} | in-fl {:>5} | incl {:>7} | TPS10 {:>6.1} \
                 | TPSavg {:>6.1} | sub p50 10s {:>3} ms / tot {:>3} \
                 | inc p50 10s {:>5.2} s / tot {:>5.2} s",
                started.elapsed().as_secs(),
                self.sent.load(Ordering::Relaxed),
                in_flight,
                inc_now,
                tps10 as f64 / 10.0,
                tps_avg,
                sub_p50_10,
                sub_p50_tot,
                inc_p50_10 as f64 / 1000.0,
                inc_p50_tot as f64 / 1000.0
            );
            std::io::stdout().flush().ok();
        }
    }
}

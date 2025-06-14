//! metrics.rs
//!
//! Shared counters + histograms + the reporter task.
//! Every second: TPS / latency lines.
//! Every 10 s  : gas-price + wallet-balance summary (ETH plus ERC-20 if a
//!               token address is supplied).

use hdrhistogram::Histogram;
use parking_lot::{Mutex, RwLock};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::time::interval;

use ethers::{
    contract::abigen,
    prelude::*,
    utils::format_units,
};

// ─── minimal ERC-20 view binding ─────────────────────────────────────── //
abigen!(
    IERC20,
    r#"[
        function balanceOf(address) view returns (uint256)
    ]"#
);

/// Median of a *sorted* slice, or 0 if empty.
fn median(vals: &[u64]) -> u64 {
    if vals.is_empty() { 0 } else { vals[vals.len() / 2] }
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

    /// Spawn the reporter task.
    /// `token_addr == None` → ETH-only summary.
    /// `token_addr == Some(addr)` → ETH + ERC-20 balances.
    pub fn spawn_reporter(
        &self,
        provider: Provider<Http>,
        wallets: Vec<Address>,
        started: Instant,
        token_addr: Option<Address>,
    ) {
        let prov_arc = Arc::new(provider.clone());
        let token = token_addr.map(|addr| IERC20::new(addr, prov_arc));

        let me = self.clone();
        tokio::spawn(async move { me.report_loop(provider, token, wallets, started).await });
    }

    async fn report_loop(
        self,
        provider: Provider<Http>,
        token: Option<IERC20<Provider<Http>>>,
        wallets: Vec<Address>,
        started: Instant,
    ) {
        let mut tick = interval(Duration::from_secs(1));
        let mut tps_q: VecDeque<(Instant, u64)> = VecDeque::new();
        let mut last_inc = 0;
        let mut since_last_health = Instant::now() - Duration::from_secs(10);

        loop {
            tick.tick().await;
            let now = Instant::now();

            // prune TPS window
            while tps_q.front().map_or(false, |(t, _)| *t + Duration::from_secs(10) < now) {
                tps_q.pop_front();
            }

            // counters
            let sent_now  = self.sent.load(Ordering::Relaxed);
            let inc_now   = self.included.load(Ordering::Relaxed);
            let delta_inc = inc_now - last_inc;
            last_inc      = inc_now;

            tps_q.push_back((now, delta_inc));
            let tps10: u64 = tps_q.iter().map(|(_, d)| *d).sum();
            let tps_avg = if started.elapsed().as_secs() == 0 {
                0.0
            } else {
                inc_now as f64 / started.elapsed().as_secs_f64()
            };

            // rolling 10-s p50
            let sub_p50_total = self.submit.read().value_at_quantile(0.5);
            let inc_p50_total = self.include.read().value_at_quantile(0.5);

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

            let in_flight = sent_now.saturating_sub(inc_now);
            println!(
                "⏱ {:>4}s | sent {:>6} | in-fl {:>4} | incl {:>6} | TPS10 {:.1} | TPSavg {:.1} | sub p50 10s {:>3} ms / tot {:>3} | inc p50 10s {:>5.2} s / tot {:>5.2} s",
                started.elapsed().as_secs(),
                sent_now,
                in_flight,
                inc_now,
                tps10 as f64 / 10.0,
                tps_avg,
                sub_p50_10,
                sub_p50_total,
                inc_p50_10 as f64 / 1000.0,
                inc_p50_total as f64 / 1000.0
            );

            // -------- health snapshot every 10 s -------- //
            if now.duration_since(since_last_health) >= Duration::from_secs(10) {
                since_last_health = now;

                // gas-price
                let gp = provider.get_gas_price().await.unwrap_or_default();
                let gp_gwei = format_units(gp, 9).unwrap_or_else(|_| "NA".into());

                // ETH stats
                let mut eth_sum = 0f64; let mut eth_min = f64::MAX; let mut eth_max = 0f64;
                // token stats
                let mut tok_sum = 0f64; let mut tok_min = f64::MAX; let mut tok_max = 0f64;

                for addr in &wallets {
                    // ETH balance
                    if let Ok(bal) = provider.get_balance(*addr, None).await {
                        if let Ok(e) = format_units(bal, 18) {
                            let v: f64 = e.parse().unwrap_or(0.0);
                            eth_sum += v; eth_min = eth_min.min(v); eth_max = eth_max.max(v);
                        }
                    }
                    // token balance
                    if let Some(ref t) = token {
                        if let Ok(bal) = t.balance_of(*addr).call().await {
                            if let Ok(s) = format_units(bal, 18) {
                                let v: f64 = s.parse().unwrap_or(0.0);
                                tok_sum += v; tok_min = tok_min.min(v); tok_max = tok_max.max(v);
                            }
                        }
                    }
                }

                let eth_avg = if wallets.is_empty() { 0.0 } else { eth_sum / wallets.len() as f64 };

                if token.is_none() {
                    println!(
                        "🔍 gas {} gwei | wallet balance (ETH) min {:.4} / avg {:.4} / max {:.4}",
                        gp_gwei.trim_end_matches('0').trim_end_matches('.'),
                        eth_min, eth_avg, eth_max
                    );
                } else {
                    let tok_avg = if wallets.is_empty() { 0.0 } else { tok_sum / wallets.len() as f64 };
                    println!(
                        "🔍 gas {} gwei | ETH min {:.4} / avg {:.4} / max {:.4} | token min {:.4} / avg {:.4} / max {:.4}",
                        gp_gwei.trim_end_matches('0').trim_end_matches('.'),
                        eth_min, eth_avg, eth_max,
                        tok_min, tok_avg, tok_max
                    );
                }
            }
        }
    }
}

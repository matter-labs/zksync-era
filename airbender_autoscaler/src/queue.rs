//! Queue report collection.
//!
//! Every chain's proof data handler exposes `GET /airbender/queue_report`
//! whose eligibility predicates must mirror the DAL locking queries exactly
//! (`lock_batch_for_proving` / `lock_batch_for_snark`): timeout reclaims,
//! attempt limits, and protocol-version filters included. The scaler counts
//! exactly what a worker could claim — nothing more, nothing less.
//!
//! A chain that fails to respond keeps its last known report up to the
//! group's staleness limit; past that the whole group is held rather than
//! scaled on partial or stale data.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::config::GroupConfig;

pub const QUEUE_REPORT_PATH: &str = "/airbender/queue_report";

/// Wire format of one chain's queue report.
#[derive(Clone, Debug, Deserialize)]
pub struct QueueReport {
    pub chain_id: u64,
    #[serde(default)]
    pub compatibility_id: Option<String>,
    pub fri: StageReport,
    pub snark: StageReport,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct StageReport {
    /// Claimable right now (fresh + failed-and-retriable).
    pub ready: u64,
    /// Currently locked by a prover and not timed out.
    pub in_progress: u64,
    /// Locked but past the processing timeout — claimable by the next fetch.
    #[serde(default)]
    pub reclaimable: u64,
    #[serde(default)]
    pub oldest_ready_at: Option<DateTime<Utc>>,
}

/// Aggregated view of one stage across every chain in a group.
#[derive(Clone, Copy, Debug, Default)]
pub struct StageQueue {
    pub ready: u64,
    pub in_progress: u64,
    pub reclaimable: u64,
    pub oldest_ready_age: Option<Duration>,
}

impl StageQueue {
    pub fn outstanding(&self) -> u64 {
        self.ready + self.in_progress + self.reclaimable
    }

    fn absorb(&mut self, report: &StageReport, now: DateTime<Utc>) {
        self.ready += report.ready;
        self.in_progress += report.in_progress;
        self.reclaimable += report.reclaimable;
        if let Some(at) = report.oldest_ready_at {
            let age = (now - at).to_std().unwrap_or_default();
            self.oldest_ready_age = Some(match self.oldest_ready_age {
                Some(existing) => existing.max(age),
                None => age,
            });
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GroupQueues {
    pub fri: StageQueue,
    pub snark: StageQueue,
}

struct CachedReport {
    report: QueueReport,
    fetched_at: Instant,
}

pub struct QueueCollector {
    client: reqwest::Client,
    cache: HashMap<String, CachedReport>,
}

impl QueueCollector {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("building HTTP client")?;
        Ok(Self {
            client,
            cache: HashMap::new(),
        })
    }

    /// Fetches every chain of the group and returns the aggregate, or `None`
    /// when any chain's data is unusable (unreachable beyond the staleness
    /// limit, or reporting a mismatched compatibility id). `None` means
    /// "hold, don't scale" — never scale a shared fleet on partial data.
    pub async fn collect(&mut self, group: &GroupConfig) -> Option<GroupQueues> {
        let now_instant = Instant::now();
        let now_utc = Utc::now();
        let mut queues = GroupQueues::default();

        for chain_url in &group.chains {
            let url = format!("{}{QUEUE_REPORT_PATH}", chain_url.trim_end_matches('/'));
            match self.fetch(&url).await {
                Ok(report) => {
                    self.cache.insert(
                        chain_url.clone(),
                        CachedReport {
                            report,
                            fetched_at: now_instant,
                        },
                    );
                }
                Err(err) => warn!(%chain_url, ?err, "failed to fetch queue report"),
            }

            let Some(cached) = self.cache.get(chain_url) else {
                warn!(%chain_url, "no queue report ever received; holding group");
                return None;
            };
            let age = now_instant.duration_since(cached.fetched_at);
            if age > group.queue_staleness_limit {
                warn!(
                    %chain_url,
                    stale_for = ?age,
                    "queue report stale beyond limit; holding group"
                );
                return None;
            }
            if let (Some(expected), Some(actual)) =
                (&group.compatibility_id, &cached.report.compatibility_id)
            {
                if expected != actual {
                    warn!(
                        %chain_url,
                        expected, actual,
                        "compatibility_id mismatch; holding group"
                    );
                    return None;
                }
            }
            debug!(
                %chain_url,
                chain_id = cached.report.chain_id,
                "absorbing queue report"
            );
            queues.fri.absorb(&cached.report.fri, now_utc);
            queues.snark.absorb(&cached.report.snark, now_utc);
        }
        Some(queues)
    }

    async fn fetch(&self, url: &str) -> Result<QueueReport> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("sending request")?
            .error_for_status()
            .context("non-success status")?;
        response.json().await.context("decoding queue report")
    }
}

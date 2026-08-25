//! Configuration for the autoscaler.
//!
//! Topology (combined `fri-snark` vs. split `fri-only` + `snark-only`) is a
//! manual, operator-level decision expressed by which deployments a group
//! declares. The autoscaler only scales replica counts within that choice.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Kubeconfig context names in fill-priority order: scale-up fills the
    /// first non-starved cluster, scale-down removes from the last one.
    /// Empty means "use the current kubeconfig context".
    #[serde(default)]
    pub clusters: Vec<String>,
    /// Namespace the prover deployments live in (same in every cluster).
    pub namespace: String,
    #[serde(with = "humantime_serde", default = "default_reconcile_interval")]
    pub reconcile_interval: Duration,
    /// Compute and log every decision but never touch Kubernetes.
    #[serde(default)]
    pub dry_run: bool,
    /// Compatibility groups (chains sharing guest binary + VKs), keyed by a
    /// short operator-chosen name used in default deployment names.
    pub groups: BTreeMap<String, GroupConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupConfig {
    /// Expected `compatibility_id` from queue reports. When set, a chain
    /// reporting a different id is treated as stale (never scaled on).
    #[serde(default)]
    pub compatibility_id: Option<String>,
    /// Base URLs of the proof data handlers of every chain in the group.
    /// The queue report is fetched from `<url>/airbender/queue_report`.
    pub chains: Vec<String>,
    /// Hard budget: total replicas across all of this group's deployments
    /// and clusters. All machines are the same (single) GPU card class.
    pub max_gpus: u32,
    /// p95 time from requesting a machine to a Running prover pod. Doubles
    /// as the starvation threshold for Pending pods.
    #[serde(with = "humantime_serde")]
    pub startup_p95: Duration,
    /// Queue reports older than this hold the group instead of scaling it.
    #[serde(with = "humantime_serde", default = "default_staleness_limit")]
    pub queue_staleness_limit: Duration,
    /// Age of the oldest ready job that bypasses scale-up confirmation.
    #[serde(with = "humantime_serde", default = "default_latency_slo")]
    pub latency_slo: Duration,
    /// Which prover deployments exist. This is the manual topology choice:
    /// combined = `fri-snark` only; split = `fri-only` + `snark-only`.
    pub deployments: BTreeMap<Mode, DeploymentConfig>,
}

/// Prover server `--mode`. Declaration order is the budget priority: when the
/// group budget cannot cover every deployment, SNARK capacity is protected
/// and `fri-only` is cut first (FRI proofs without a wrapper just pile up).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    SnarkOnly,
    FriSnark,
    FriOnly,
}

impl Mode {
    pub fn as_kebab(&self) -> &'static str {
        match self {
            Mode::SnarkOnly => "snark-only",
            Mode::FriSnark => "fri-snark",
            Mode::FriOnly => "fri-only",
        }
    }

    /// Which queue stage this deployment drains. `fri-snark` claims FRI jobs
    /// (its SNARK work is a local follow-up), so it scales on the FRI queue.
    pub fn stage(&self) -> Stage {
        match self {
            Mode::SnarkOnly => Stage::Snark,
            Mode::FriSnark | Mode::FriOnly => Stage::Fri,
        }
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_kebab())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Fri,
    Snark,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentConfig {
    /// Kubernetes Deployment name; defaults to `airbender-<mode>-<group>`.
    /// The Deployment must already exist (created by helm/manifests, possibly
    /// at 0 replicas) — the autoscaler only patches its scale.
    #[serde(default)]
    pub name: Option<String>,
    /// Conservative p95 duration of one job for this deployment's stage
    /// (for `fri-snark`: FRI + local SNARK wrap combined).
    #[serde(with = "humantime_serde")]
    pub duration_p95: Duration,
    /// Target horizon to drain the outstanding work within.
    #[serde(with = "humantime_serde")]
    pub drain_window: Duration,
    /// Warm minimum kept even with an empty queue.
    #[serde(default)]
    pub min: u32,
    pub max: u32,
    /// Consecutive over-threshold observations required before scaling up
    /// (bypassed when the oldest ready job breaches `latency_slo`).
    #[serde(default = "default_scale_up_confirmations")]
    pub scale_up_confirmations: u32,
    /// Sustained low-demand period required before removing one replica.
    #[serde(with = "humantime_serde", default = "default_scale_down_stabilization")]
    pub scale_down_stabilization: Duration,
}

impl DeploymentConfig {
    pub fn resolved_name(&self, mode: Mode, group: &str) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("airbender-{}-{group}", mode.as_kebab()))
    }
}

fn default_reconcile_interval() -> Duration {
    Duration::from_secs(20)
}

fn default_staleness_limit() -> Duration {
    Duration::from_secs(5 * 60)
}

fn default_latency_slo() -> Duration {
    Duration::from_secs(4 * 3600)
}

fn default_scale_up_confirmations() -> u32 {
    2
}

fn default_scale_down_stabilization() -> Duration {
    Duration::from_secs(25 * 60)
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config from {}", path.display()))?;
        let config: Config = serde_yaml::from_str(&raw)
            .with_context(|| format!("parsing config from {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.groups.is_empty() {
            bail!("config declares no groups");
        }
        for (name, group) in &self.groups {
            group
                .validate()
                .with_context(|| format!("in group `{name}`"))?;
        }
        Ok(())
    }
}

impl GroupConfig {
    fn validate(&self) -> Result<()> {
        if self.chains.is_empty() {
            bail!("group has no chain URLs");
        }
        if self.deployments.is_empty() {
            bail!("group declares no deployments");
        }
        // The only stranding misconfiguration: FRI proofs would be produced
        // but nothing SNARK-capable exists to wrap them.
        let has_fri_only = self.deployments.contains_key(&Mode::FriOnly);
        let has_snark_capable = self.deployments.contains_key(&Mode::SnarkOnly)
            || self.deployments.contains_key(&Mode::FriSnark);
        if has_fri_only && !has_snark_capable {
            bail!(
                "`fri-only` is configured without any SNARK-capable deployment \
                 (`snark-only` or `fri-snark`); FRI proofs would never be wrapped"
            );
        }
        let mut min_total: u32 = 0;
        for (mode, dep) in &self.deployments {
            if dep.max < dep.min {
                bail!("deployment `{mode}`: max ({}) < min ({})", dep.max, dep.min);
            }
            if dep.scale_up_confirmations == 0 {
                bail!("deployment `{mode}`: scale_up_confirmations must be >= 1");
            }
            if dep.drain_window.is_zero() || dep.duration_p95.is_zero() {
                bail!("deployment `{mode}`: duration_p95 and drain_window must be non-zero");
            }
            min_total = min_total.saturating_add(dep.min);
        }
        if min_total > self.max_gpus {
            bail!(
                "sum of deployment minimums ({min_total}) exceeds max_gpus ({})",
                self.max_gpus
            );
        }
        Ok(())
    }
}

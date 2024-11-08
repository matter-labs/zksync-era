use std::{collections::HashMap, path::PathBuf, time::Duration};

use anyhow::Context;
use serde::Deserialize;
use strum::Display;
use strum_macros::EnumString;
use vise::EncodeLabelValue;
use zksync_config::configs::ObservabilityConfig;

/// Config used for running ProverAutoscaler (both Scaler and Agent).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProverAutoscalerConfig {
    /// Amount of time ProverJobMonitor will wait all it's tasks to finish.
    #[serde(with = "humantime_serde")]
    pub graceful_shutdown_timeout: Duration,
    pub agent_config: Option<ProverAutoscalerAgentConfig>,
    pub scaler_config: Option<ProverAutoscalerScalerConfig>,
    pub observability: Option<ObservabilityConfig>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProverAutoscalerAgentConfig {
    /// Port for prometheus metrics connection.
    pub prometheus_port: u16,
    /// HTTP port for global Scaler to connect to the Agent running in a cluster.
    pub http_port: u16,
    /// List of namespaces to watch.
    #[serde(default = "ProverAutoscalerAgentConfig::default_namespaces")]
    pub namespaces: Vec<String>,
    /// If dry-run enabled don't do any k8s updates, just report success.
    #[serde(default = "ProverAutoscalerAgentConfig::default_dry_run")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct ProverAutoscalerScalerConfig {
    /// Port for prometheus metrics connection.
    pub prometheus_port: u16,
    /// The interval between runs for global Scaler.
    #[serde(
        with = "humantime_serde",
        default = "ProverAutoscalerScalerConfig::default_scaler_run_interval"
    )]
    pub scaler_run_interval: Duration,
    /// URL to get queue reports from.
    /// In production should be "http://prover-job-monitor.stage2.svc.cluster.local:3074/queue_report".
    #[serde(default = "ProverAutoscalerScalerConfig::default_prover_job_monitor_url")]
    pub prover_job_monitor_url: String,
    /// List of ProverAutoscaler Agents to get cluster data from.
    pub agents: Vec<String>,
    /// Mapping of namespaces to protocol versions.
    pub protocol_versions: HashMap<String, String>,
    /// Default priorities, which cluster to prefer when there is no other information.
    pub cluster_priorities: HashMap<String, u32>,
    /// Prover speed per GPU. Used to calculate desired number of provers for queue size.
    pub prover_speed: HashMap<Gpu, u32>,
    /// Maximum number of provers which can be run per cluster/GPU.
    pub max_provers: HashMap<String, HashMap<Gpu, u32>>,
    /// Minimum number of provers per namespace.
    pub min_provers: HashMap<String, u32>,
    /// Duration after which pending pod considered long pending.
    #[serde(
        with = "humantime_serde",
        default = "ProverAutoscalerScalerConfig::default_long_pending_duration"
    )]
    pub long_pending_duration: Duration,
    /// List of simple autoscaler targets.
    pub scaler_targets: Vec<ScalerTarget>,
    /// If dry-run enabled don't send any scale requests.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(
    Default,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    Clone,
    Copy,
    Ord,
    PartialOrd,
    EnumString,
    EncodeLabelValue,
    Deserialize,
)]
pub enum Gpu {
    #[default]
    Unknown,
    #[strum(ascii_case_insensitive)]
    L4,
    #[strum(ascii_case_insensitive)]
    T4,
    #[strum(ascii_case_insensitive)]
    V100,
    #[strum(ascii_case_insensitive)]
    P100,
    #[strum(ascii_case_insensitive)]
    A100,
}

// TODO: generate this enum by QueueReport from https://github.com/matter-labs/zksync-era/blob/main/prover/crates/bin/prover_job_monitor/src/autoscaler_queue_reporter.rs#L23
// and remove allowing of non_camel_case_types by generating field name parser.
#[derive(Debug, Display, PartialEq, Eq, Hash, Clone, Copy, Deserialize, EnumString, Default)]
#[allow(non_camel_case_types)]
pub enum QueueReportFields {
    #[strum(ascii_case_insensitive)]
    basic_witness_jobs,
    #[strum(ascii_case_insensitive)]
    leaf_witness_jobs,
    #[strum(ascii_case_insensitive)]
    node_witness_jobs,
    #[strum(ascii_case_insensitive)]
    recursion_tip_witness_jobs,
    #[strum(ascii_case_insensitive)]
    scheduler_witness_jobs,
    #[strum(ascii_case_insensitive)]
    proof_compressor_jobs,
    #[default]
    #[strum(ascii_case_insensitive)]
    prover_jobs,
}

/// ScalerTarget can be configured to autoscale any of services for which queue is reported by
/// prover-job-monitor, except of provers. Provers need special treatment due to GPU requirement.
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct ScalerTarget {
    pub queue_report_field: QueueReportFields,
    pub deployment: String,
    /// Max replicas per cluster.
    pub max_replicas: HashMap<String, usize>,
    /// The queue will be divided by the speed and rounded up to get number of replicas.
    #[serde(default = "ScalerTarget::default_speed")]
    pub speed: usize,
}

impl ProverAutoscalerConfig {
    /// Default graceful shutdown timeout -- 5 seconds
    pub fn default_graceful_shutdown_timeout() -> Duration {
        Duration::from_secs(5)
    }
}

impl ProverAutoscalerAgentConfig {
    pub fn default_namespaces() -> Vec<String> {
        vec!["prover-blue".to_string(), "prover-red".to_string()]
    }

    pub fn default_dry_run() -> bool {
        true
    }
}

impl ProverAutoscalerScalerConfig {
    /// Default scaler_run_interval -- 10s
    pub fn default_scaler_run_interval() -> Duration {
        Duration::from_secs(10)
    }

    /// Default prover_job_monitor_url -- cluster local URL
    pub fn default_prover_job_monitor_url() -> String {
        "http://localhost:3074/queue_report".to_string()
    }

    /// Default long_pending_duration -- 10m
    pub fn default_long_pending_duration() -> Duration {
        Duration::from_secs(600)
    }
}

impl ScalerTarget {
    pub fn default_speed() -> usize {
        1
    }
}

pub fn config_from_yaml<T: serde::de::DeserializeOwned>(path: &PathBuf) -> anyhow::Result<T> {
    let yaml = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(serde_yaml::from_str(&yaml)?)
}

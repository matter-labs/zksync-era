use std::collections::HashMap;

use serde::Deserialize;
use strum::Display;
use strum_macros::EnumString;
use time::Duration;
use vise::EncodeLabelValue;

use crate::configs::ObservabilityConfig;

/// Config used for running ProverAutoscaler (both Scaler and Agent).
#[derive(Debug, Clone, PartialEq)]
pub struct ProverAutoscalerConfig {
    /// Amount of time ProverJobMonitor will wait all it's tasks to finish.
    // TODO: find a way to use #[serde(with = "humantime_serde")] with time::Duration.
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
    /// Watched cluster name. Also can be set via flag.
    pub cluster_name: Option<String>,
    /// If dry-run enabled don't do any k8s updates, just report success.
    #[serde(default = "ProverAutoscalerAgentConfig::default_dry_run")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct ProverAutoscalerScalerConfig {
    /// Port for prometheus metrics connection.
    pub prometheus_port: u16,
    /// The interval between runs for global Scaler.
    #[serde(default = "ProverAutoscalerScalerConfig::default_scaler_run_interval")]
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
    #[serde(default = "ProverAutoscalerScalerConfig::default_long_pending_duration")]
    pub long_pending_duration: Duration,
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

impl ProverAutoscalerConfig {
    /// Default graceful shutdown timeout -- 5 seconds
    pub fn default_graceful_shutdown_timeout() -> Duration {
        Duration::seconds(5)
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
        Duration::seconds(10)
    }

    /// Default prover_job_monitor_url -- cluster local URL
    pub fn default_prover_job_monitor_url() -> String {
        "http://localhost:3074/queue_report".to_string()
    }

    /// Default long_pending_duration -- 10m
    pub fn default_long_pending_duration() -> Duration {
        Duration::minutes(10)
    }
}

use std::{collections::HashMap, hash::Hash, path::PathBuf, time::Duration};

use anyhow::Context;
use serde::Deserialize;
use strum::Display;
use strum_macros::EnumString;
use zksync_config::configs::ObservabilityConfig;

use crate::{
    cluster_types::{ClusterName, DeploymentName, NamespaceName},
    key::{GpuKey, NoKey},
};

/// Config used for running ProverAutoscaler (both Scaler and Agent).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProverAutoscalerConfig {
    /// Amount of time ProverJobMonitor will wait all it's tasks to finish.
    #[serde(
        with = "humantime_serde",
        default = "ProverAutoscalerConfig::default_graceful_shutdown_timeout"
    )]
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
    pub namespaces: Vec<NamespaceName>,
    /// If dry-run enabled don't do any k8s updates, just report success.
    #[serde(default = "ProverAutoscalerAgentConfig::default_dry_run")]
    pub dry_run: bool,
    /// Interval for periodic pod checks against the K8s API to remove stale pods.
    #[serde(
        with = "humantime_serde",
        default = "ProverAutoscalerAgentConfig::default_pod_check_interval"
    )]
    pub pod_check_interval: Duration,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
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
    /// List of ProverAutoscaler Agent base URLs to get cluster data from.
    pub agents: Vec<String>,
    /// Mapping of namespaces to protocol versions.
    pub protocol_versions: HashMap<NamespaceName, String>,
    /// Default priorities, which cluster to prefer when there is no other information.
    pub cluster_priorities: HashMap<ClusterName, u32>,
    /// Prover speed per GPU. Used to calculate desired number of provers for queue size.
    pub apply_min_to_namespace: Option<NamespaceName>,
    /// Duration after which pending pod considered long pending.
    #[serde(
        with = "humantime_serde",
        default = "ProverAutoscalerScalerConfig::default_long_pending_duration"
    )]
    pub long_pending_duration: Duration,
    /// Time window for including scale errors in Autoscaler calculations. Clusters will be sorted by number of the errors.
    #[serde(
        with = "humantime_serde",
        default = "ProverAutoscalerScalerConfig::default_scale_errors_duration"
    )]
    pub scale_errors_duration: Duration,
    /// List of simple autoscaler targets.
    pub scaler_targets: Vec<ScalerTarget>,
    /// If dry-run enabled don't send any scale requests.
    #[serde(default)]
    pub dry_run: bool,
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ScalarOrMap {
    Scalar(usize),
    Map(HashMap<GpuKey, usize>),
}
impl ScalarOrMap {
    pub fn into_map_nokey(&self) -> HashMap<NoKey, usize> {
        match self {
            ScalarOrMap::Scalar(x) => [(NoKey::default(), *x)].into(),
            ScalarOrMap::Map(m) => panic!("Passed GpuKey in NoKey context! {:?}", m),
        }
    }

    pub fn into_map_gpukey(&self) -> HashMap<GpuKey, usize> {
        match self {
            ScalarOrMap::Scalar(x) => [(GpuKey::default(), *x)].into(),
            ScalarOrMap::Map(m) => m.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum PriorityConfig {
    Gpu(Vec<(ClusterName, GpuKey)>),
    Simple(Vec<ClusterName>),
}

#[derive(Debug, Default, Display, Clone, Copy, PartialEq, EnumString, Deserialize)]
pub enum ScalerTargetType {
    #[default]
    #[serde(alias = "simple")]
    Simple,
    #[serde(alias = "gpu", alias = "GPU")]
    Gpu,
}

/// ScalerTarget can be configured to autoscale any of services for which queue is reported by
/// prover-job-monitor.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ScalerTarget {
    #[serde(default)]
    pub scaler_target_type: ScalerTargetType,
    pub queue_report_field: QueueReportFields,
    pub deployment: DeploymentName,
    /// Min replicas globally.
    #[serde(default)]
    pub min_replicas: usize,
    /// Max replicas per cluster.
    pub max_replicas: HashMap<ClusterName, ScalarOrMap>,
    /// The queue will be divided by the speed and rounded up to get number of replicas.
    #[serde(default = "ScalerTarget::default_speed")]
    pub speed: ScalarOrMap,
    /// Optional priority list that overrides global cluster_priorities.
    /// For GPU targets, this is a list of (ClusterName, GpuKey) tuples.
    /// For Simple targets, this is a list of ClusterName.
    #[serde(default)]
    pub priority: Option<PriorityConfig>,
}

impl ProverAutoscalerConfig {
    /// Default graceful shutdown timeout -- 5 seconds
    pub fn default_graceful_shutdown_timeout() -> Duration {
        Duration::from_secs(5)
    }
}

impl ProverAutoscalerAgentConfig {
    pub fn default_namespaces() -> Vec<NamespaceName> {
        vec!["prover-blue".into(), "prover-red".into()]
    }

    pub fn default_dry_run() -> bool {
        true
    }

    pub fn default_pod_check_interval() -> Duration {
        Duration::from_secs(3600) // Default to 1 hour
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

    /// Default long_pending_duration -- 1h
    pub fn default_scale_errors_duration() -> Duration {
        Duration::from_secs(3600)
    }
}

impl ScalerTarget {
    pub fn default_speed() -> ScalarOrMap {
        ScalarOrMap::Scalar(1)
    }
}

pub fn config_from_yaml<T: serde::de::DeserializeOwned>(path: &PathBuf) -> anyhow::Result<T> {
    let yaml = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(serde_yaml::from_str(&yaml)?)
}

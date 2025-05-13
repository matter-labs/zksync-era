use std::{collections::HashMap, hash::Hash, time::Duration};

use serde::{Deserialize, Serialize};
use smart_config::{
    de::{Serde, WellKnown},
    metadata::TimeUnit,
    DescribeConfig, DeserializeConfig,
};
use strum::Display;
use strum_macros::EnumString;

use crate::{
    cluster_types::{ClusterName, DeploymentName, NamespaceName},
    key::{GpuKey, NoKey},
};

impl WellKnown for NamespaceName {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

impl WellKnown for ClusterName {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

/// Config used for running ProverAutoscaler (both Scaler and Agent).
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ProverAutoscalerConfig {
    /// Amount of time ProverJobMonitor will wait all it's tasks to finish.
    #[config(default_t = Duration::from_secs(5))]
    pub graceful_shutdown_timeout: Duration,
    #[config(nest)]
    pub agent_config: Option<ProverAutoscalerAgentConfig>,
    #[config(nest)]
    pub scaler_config: Option<ProverAutoscalerScalerConfig>,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ProverAutoscalerAgentConfig {
    /// Port for prometheus metrics connection.
    pub prometheus_port: u16,
    /// HTTP port for global Scaler to connect to the Agent running in a cluster.
    pub http_port: u16,
    /// List of namespaces to watch.
    #[config(default_t = vec!["prover-blue".into(), "prover-red".into()])]
    pub namespaces: Vec<NamespaceName>,
    /// If dry-run enabled don't do any k8s updates, just report success.
    #[config(default_t = true)]
    pub dry_run: bool,
    /// Interval for periodic pod checks against the K8s API to remove stale pods.
    #[config(default_t = 1 * TimeUnit::Hours)]
    pub pod_check_interval: Duration,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ProverAutoscalerScalerConfig {
    /// Port for prometheus metrics connection.
    pub prometheus_port: u16,
    /// The interval between runs for global Scaler.
    #[config(default_t = Duration::from_secs(10))]
    pub scaler_run_interval: Duration,
    /// URL to get queue reports from.
    /// In production should be "http://prover-job-monitor.stage2.svc.cluster.local:3074/queue_report".
    #[config(default_t = "http://localhost:3074/queue_report".to_string())]
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
    #[config(default_t = 10 * TimeUnit::Minutes)]
    pub long_pending_duration: Duration,
    /// Time window for including scale errors in Autoscaler calculations. Clusters will be sorted by number of the errors.
    #[config(default_t = 1 * TimeUnit::Hours)]
    pub scale_errors_duration: Duration,
    /// Time window for which Autoscaler forces pending pod migration due to scale errors.
    #[config(default_t = 4 * TimeUnit::Minutes)]
    pub need_to_move_duration: Duration,
    /// List of simple autoscaler targets.
    pub scaler_targets: Vec<ScalerTarget>,
    /// If dry-run enabled don't send any scale requests.
    #[config(default)]
    pub dry_run: bool,
}

// TODO: generate this enum by QueueReport from https://github.com/matter-labs/zksync-era/blob/main/prover/crates/bin/prover_job_monitor/src/autoscaler_queue_reporter.rs#L23
// and remove allowing of non_camel_case_types by generating field name parser.
#[derive(
    Debug, Display, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize, EnumString, Default,
)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Default, Display, Clone, Copy, PartialEq, EnumString, Serialize, Deserialize)]
pub enum ScalerTargetType {
    #[default]
    #[serde(alias = "simple")]
    Simple,
    #[serde(alias = "gpu", alias = "GPU")]
    Gpu,
}

/// ScalerTarget can be configured to autoscale any of services for which queue is reported by
/// prover-job-monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

impl WellKnown for ScalerTarget {
    type Deserializer = Serde![object];
    const DE: Self::Deserializer = Serde![object];
}

impl ScalerTarget {
    pub fn default_speed() -> ScalarOrMap {
        ScalarOrMap::Scalar(1)
    }
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    #[test]
    fn deserializing_config() {
        let yaml = r#"
            graceful_shutdown_timeout: 10s
            agent_config:
              prometheus_port: 8080
              http_port: 8081
              namespaces:
                - prover-blue
                - prover-red
              pod_check_interval: 10m
              dry_run: false
            scaler_config:
              dry_run: false
              prometheus_port: 8080
              prover_job_monitor_url: http://prover_job_monitor_url/queue_report
              agents:
                - http://prover-autoscaler/
              scaler_run_interval: 60s
              protocol_versions:
                prover-blue: 0.26.0
                prover-red: 0.27.0
              cluster_priorities:
                zksync-era-gateway-stage: 0
              apply_min_to_namespace: prover-blue
              long_pending_duration: 10m
              scale_errors_duration: 2h
              need_to_move_duration: 5m
              scaler_targets:
                - queue_report_field: prover_jobs
                  scaler_target_type: gpu
                  deployment: circuit-prover-gpu
                  min_replicas: 0
                  max_replicas:
                    zksync-era-gateway-stage:
                      # Quota: 254
                      L4: 256
                      # Quota: 256
                      T4: 256
                  speed:
                    L4: 1500
                    T4: 700
                - queue_report_field: basic_witness_jobs
                  deployment: witness-generator-basic-fri
                  max_replicas:
                    zksync-era-gateway-stage: 150
                  speed: 4
                - queue_report_field: leaf_witness_jobs
                  deployment: witness-generator-leaf-fri
                  max_replicas:
                    zksync-era-gateway-stage: 150
                  speed: 10
                - queue_report_field: node_witness_jobs
                  deployment: witness-generator-node-fri
                  max_replicas:
                    zksync-era-gateway-stage: 150
                  speed: 10
                - queue_report_field: recursion_tip_witness_jobs
                  deployment: witness-generator-recursion-tip-fri
                  max_replicas:
                    zksync-era-gateway-stage: 150
                  speed: 10
                - queue_report_field: scheduler_witness_jobs
                  deployment: witness-generator-scheduler-fri
                  max_replicas:
                    zksync-era-gateway-stage: 150
                  speed: 10
                - queue_report_field: proof_compressor_jobs
                  deployment: proof-fri-gpu-compressor
                  max_replicas:
                    zksync-era-gateway-stage: 100
                  speed: 5
        "#;
        let yaml = serde_yaml::from_str(yaml).unwrap();
        let yaml = Yaml::new("test.yml", yaml).unwrap();

        let config: ProverAutoscalerConfig = test_complete(yaml).unwrap();
        assert_eq!(config.graceful_shutdown_timeout, Duration::from_secs(10));

        let agent_config = config.agent_config.unwrap();
        assert_eq!(agent_config.prometheus_port, 8_080);
        assert_eq!(agent_config.http_port, 8_081);
        assert_eq!(
            agent_config.namespaces,
            ["prover-blue".into(), "prover-red".into()]
        );
        assert_eq!(agent_config.pod_check_interval, Duration::from_secs(600));
        assert!(!agent_config.dry_run);

        let scaler_config = config.scaler_config.unwrap();
        assert_eq!(scaler_config.prometheus_port, 8_080);
        assert_eq!(scaler_config.scaler_run_interval, Duration::from_secs(60));
        assert_eq!(
            scaler_config.scale_errors_duration,
            Duration::from_secs(7_200)
        );
        assert_eq!(
            scaler_config.need_to_move_duration,
            Duration::from_secs(5 * 60)
        );
        assert_eq!(
            scaler_config.protocol_versions,
            HashMap::from([
                ("prover-blue".into(), "0.26.0".into()),
                ("prover-red".into(), "0.27.0".into())
            ])
        );
        assert_eq!(
            scaler_config.apply_min_to_namespace.unwrap(),
            "prover-blue".into()
        );
        assert_eq!(
            scaler_config.long_pending_duration,
            Duration::from_secs(600)
        );
        assert_eq!(scaler_config.scaler_targets.len(), 7);
    }
}

use serde::Deserialize;
use time::Duration;

/// Config used for running ProverAutoscaler (both Scaler and Agent).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GeneralConfig {
    /// Amount of time ProverJobMonitor will wait all it's tasks to finish.
    // TODO: find a way to use #[serde(with = "humantime_serde")] with time::Duration.
    #[serde(default = "GeneralConfig::default_graceful_shutdown_timeout")]
    pub graceful_shutdown_timeout: Duration,
    pub agent_config: Option<ProverAutoscalerAgentConfig>,
    pub scaler_config: Option<ProverAutoscalerScalerConfig>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProverAutoscalerAgentConfig {
    /// Port for prometheus metrics connection.
    pub prometheus_port: u16,
    /// HTTP port for Scaler to connect to.
    pub http_port: u16,
    /// List of namespaces to watch.
    #[serde(default = "ProverAutoscalerAgentConfig::default_namespaces")]
    pub namespaces: Vec<String>,
    /// Watched cluster name. Also can be set via flag.
    pub cluster_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProverAutoscalerScalerConfig {
    /// Port for prometheus metrics connection.
    pub prometheus_port: u16,
    /// The interval between runs for GPU Prover Archiver.
    #[serde(default = "ProverAutoscalerScalerConfig::default_scaler_run_interval")]
    pub scaler_run_interval: Duration,
    /// URL to get queue reports from.
    #[serde(default = "ProverAutoscalerScalerConfig::default_prover_job_monitor_url")]
    pub prover_job_monitor_url: String,
    /// List of ProverAutoscaler Agents to get data from.
    pub agents: Vec<String>,
}

impl GeneralConfig {
    /// Default graceful shutdown timeout -- 5 seconds
    pub fn default_graceful_shutdown_timeout() -> Duration {
        Duration::seconds(5)
    }
}

impl ProverAutoscalerAgentConfig {
    pub fn default_namespaces() -> Vec<String> {
        vec!["prover-blue".to_string(), "prover-red".to_string()]
    }
}

impl ProverAutoscalerScalerConfig {
    /// Default scaler_run_interval -- 10s
    pub fn default_scaler_run_interval() -> Duration {
        Duration::seconds(10)
    }

    /// Default prover_job_monitor_url -- cluster local URL
    pub fn default_prover_job_monitor_url() -> String {
        "http://prover-job-monitor.stage2.svc.cluster.local:3074/queue_report".to_string()
    }
}

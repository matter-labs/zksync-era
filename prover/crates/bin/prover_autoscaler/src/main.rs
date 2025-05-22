use anyhow::Context;
use smart_config::{ConfigSchema, DescribeConfig};
use structopt::StructOpt;
use tokio::sync::{oneshot, watch};

use zksync_config::{sources::ConfigSources, ConfigRepositoryExt};
use zksync_prover_autoscaler::{
    agent,
    cluster_types::ClusterName,
    config::ProverAutoscalerConfig,
    global::{manager::Manager, queuer::Queuer, watcher},
    http_client::HttpClient,
    k8s::{Scaler, Watcher},
};
use zksync_prover_task::TaskRunner;
use zksync_task_management::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

/// Represents the sequential number of the Prover Autoscaler type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AutoscalerType {
    Scaler,
    Agent,
}

impl std::str::FromStr for AutoscalerType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "scaler" => Ok(AutoscalerType::Scaler),
            "agent" => Ok(AutoscalerType::Agent),
            other => Err(format!("{} is not a valid AutoscalerType", other)),
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "Prover Autoscaler", about = "Run Prover Autoscaler components")]
struct Opt {
    /// Prover Autoscaler can run Agent or Scaler type.
    ///
    /// Specify `agent` or `scaler`
    #[structopt(short, long, default_value = "agent")]
    job: AutoscalerType,
    /// Name of the cluster Agent is watching.
    #[structopt(long)]
    cluster_name: Option<ClusterName>,
    /// Path to the configuration file.
    #[structopt(long)]
    config_path: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    let config_sources = ConfigSources::default().with_yaml(&opt.config_path)?;
    let _observability_guard = config_sources.observability()?.install()?;

    let full_config_schema = ConfigSchema::new(&ProverAutoscalerConfig::DESCRIPTION, "");
    let config_repo = config_sources.build_repository(&full_config_schema);
    let general_config: ProverAutoscalerConfig = config_repo.parse()?;

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(sender) = stop_signal_sender.take() {
            sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    let (stop_sender, stop_receiver) = watch::channel(false);

    let mut tasks = vec![];

    let http_client = HttpClient::default();

    match opt.job {
        AutoscalerType::Agent => {
            tracing::info!("Starting ProverAutoscaler Agent");
            let agent_config = general_config.agent_config.context("agent_config")?;
            let exporter_config = PrometheusExporterConfig::pull(agent_config.prometheus_port);
            tasks.push(tokio::spawn(exporter_config.run(stop_receiver.clone())));

            let _ = rustls::crypto::ring::default_provider().install_default();
            let client = kube::Client::try_default().await?;

            let watcher = Watcher::new(
                http_client,
                client.clone(),
                opt.cluster_name,
                agent_config.namespaces,
                agent_config.pod_check_interval,
            )
            .await;
            let scaler = Scaler::new(client, agent_config.dry_run);
            tasks.push(tokio::spawn(watcher.clone().run(stop_receiver.clone())));
            tasks.push(tokio::spawn(agent::run_server(
                agent_config.http_port,
                watcher,
                scaler,
                stop_receiver.clone(),
            )))
        }
        AutoscalerType::Scaler => {
            tracing::info!("Starting ProverAutoscaler Scaler");
            let scaler_config = general_config.scaler_config.context("scaler_config")?;
            let interval = scaler_config.scaler_run_interval;
            let exporter_config = PrometheusExporterConfig::pull(scaler_config.prometheus_port);
            tasks.push(tokio::spawn(exporter_config.run(stop_receiver.clone())));

            let watcher = watcher::Watcher::new(
                http_client.clone(),
                scaler_config.agents.clone(),
                scaler_config.dry_run,
            );
            let queuer = Queuer::new(http_client, scaler_config.prover_job_monitor_url.clone());
            let manager = Manager::new(watcher.clone(), queuer, scaler_config.clone());

            let mut task_runner = TaskRunner::default();
            task_runner.extend("AgentPoller", interval, watcher.create_poller_tasks());
            task_runner.add("Manager", interval, manager);
            tasks.extend(task_runner.spawn(stop_receiver));
        }
    }

    let mut tasks = ManagedTasks::new(tasks);

    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop request received, shutting down");
        }
    }
    stop_sender.send(true).ok();
    tasks
        .complete(general_config.graceful_shutdown_timeout)
        .await;

    Ok(())
}

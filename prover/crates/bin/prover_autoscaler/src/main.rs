use std::time::Duration;

use anyhow::Context;
use smart_config::{ConfigRepository, ConfigSchema, ConfigSources, DescribeConfig};
use structopt::StructOpt;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};
use zksync_config::{configs::ObservabilityConfig, sources::ConfigFilePaths, ParseResultExt};
use zksync_prover_autoscaler::{
    agent,
    cluster_types::ClusterName,
    config::ProverAutoscalerConfig,
    global::{manager::Manager, queuer::Queuer, watcher},
    http_client::HttpClient,
    k8s::{Scaler, Watcher},
    task_wiring::TaskRunner,
};
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

    let mut config_sources = ConfigSources::default();
    config_sources.push(ConfigFilePaths::read_yaml(&opt.config_path)?);

    let observability_config =
        ObservabilityConfig::from_sources(config_sources.clone()).context("ObservabilityConfig")?;
    let _observability_guard = observability_config.install()?;

    let full_config_schema = ConfigSchema::new(&ProverAutoscalerConfig::DESCRIPTION, "");
    let config_repo = ConfigRepository::new(&full_config_schema).with_all(config_sources);
    let general_config: ProverAutoscalerConfig = config_repo.single()?.parse().log_all_errors()?;

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
            )
            .await;
            let scaler = Scaler::new(client, agent_config.dry_run);
            tasks.push(tokio::spawn(watcher.clone().run()));
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
            let manager = Manager::new(watcher.clone(), queuer, scaler_config);
            tasks.extend(get_tasks(watcher, manager, interval, stop_receiver)?);
        }
    }

    let mut tasks = ManagedTasks::new(tasks);

    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        }
    }
    stop_sender.send(true).ok();
    tasks
        .complete(general_config.graceful_shutdown_timeout)
        .await;

    Ok(())
}

fn get_tasks(
    watcher: watcher::Watcher,
    manager: Manager,
    interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> {
    let mut task_runner = TaskRunner::default();

    task_runner.add("Watcher", interval, watcher);
    task_runner.add("Scaler", interval, manager);

    Ok(task_runner.spawn(stop_receiver))
}

use std::time::Duration;

use anyhow::Context;
use structopt::StructOpt;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};
use zksync_core_leftovers::temp_config_store::read_yaml_repr;
use zksync_protobuf_config::proto::prover_autoscaler;
use zksync_prover_autoscaler::{
    agent,
    global::{self},
    k8s::{Scaler, Watcher},
    task_wiring::TaskRunner,
};
use zksync_utils::wait_for_tasks::ManagedTasks;
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
    cluster_name: Option<String>,
    /// Path to the configuration file.
    #[structopt(long)]
    config_path: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let general_config =
        read_yaml_repr::<prover_autoscaler::ProverAutoscalerConfig>(&opt.config_path)
            .context("general config")?;
    let observability_config = general_config
        .observability
        .context("observability config")?;
    let _observability_guard = observability_config.install()?;
    // That's unfortunate that there are at least 3 different Duration in rust and we use all 3 in this repo.
    // TODO: Consider updating zksync_protobuf to support std::time::Duration.
    let graceful_shutdown_timeout = general_config.graceful_shutdown_timeout.unsigned_abs();

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(sender) = stop_signal_sender.take() {
            sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    let (stop_sender, stop_receiver) = watch::channel(false);

    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = kube::Client::try_default().await?;

    let mut tasks = vec![];

    match opt.job {
        AutoscalerType::Agent => {
            let cluster = opt
                .cluster_name
                .context("cluster_name is required for Agent")?;
            tracing::info!("Starting ProverAutoscaler Agent for cluster {}", cluster);
            let agent_config = general_config.agent_config.context("agent_config")?;
            let exporter_config = PrometheusExporterConfig::pull(agent_config.prometheus_port);
            tasks.push(tokio::spawn(exporter_config.run(stop_receiver.clone())));

            // TODO: maybe get cluster name from curl -H "Metadata-Flavor: Google"
            // http://metadata.google.internal/computeMetadata/v1/instance/attributes/cluster-name
            let watcher = Watcher::new(client.clone(), cluster, agent_config.namespaces);
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
            let interval = scaler_config.scaler_run_interval.unsigned_abs();
            let exporter_config = PrometheusExporterConfig::pull(scaler_config.prometheus_port);
            tasks.push(tokio::spawn(exporter_config.run(stop_receiver.clone())));
            let watcher =
                global::watcher::Watcher::new(scaler_config.agents.clone(), scaler_config.dry_run);
            let queuer = global::queuer::Queuer::new(scaler_config.prover_job_monitor_url.clone());
            let scaler = global::scaler::Scaler::new(watcher.clone(), queuer, scaler_config);
            tasks.extend(get_tasks(watcher, scaler, interval, stop_receiver)?);
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
    tasks.complete(graceful_shutdown_timeout).await;

    Ok(())
}

fn get_tasks(
    watcher: global::watcher::Watcher,
    scaler: global::scaler::Scaler,
    interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> {
    let mut task_runner = TaskRunner::default();

    task_runner.add("Watcher", interval, watcher);
    task_runner.add("Scaler", interval, scaler);

    Ok(task_runner.spawn(stop_receiver))
}

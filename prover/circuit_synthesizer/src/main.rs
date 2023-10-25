use anyhow::Context as _;
use prometheus_exporter::PrometheusExporterConfig;
use structopt::StructOpt;
use tokio::{sync::oneshot, sync::watch};

use zksync_config::configs::{AlertsConfig, CircuitSynthesizerConfig, FromEnv, ProverGroupConfig};
use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStoreFactory;
use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::wait_for_tasks;
use zksync_verification_key_server::get_cached_commitments;

use crate::circuit_synthesizer::CircuitSynthesizer;

mod circuit_synthesizer;

#[derive(Debug, StructOpt)]
#[structopt(name = "TODO", about = "TODO")]
struct Opt {
    /// Number of times circuit_synthesizer should be run.
    #[structopt(short = "n", long = "n_iterations")]
    number_of_iterations: Option<usize>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let sentry_url = vlog::sentry_url_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let environment = vlog::environment_from_env();

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = sentry_url {
        builder = builder
            .with_sentry_url(&sentry_url)
            .context("Invalid Sentry URL")?
            .with_sentry_environment(environment);
    }
    let _guard = builder.build();

    let opt = Opt::from_args();
    let config: CircuitSynthesizerConfig =
        CircuitSynthesizerConfig::from_env().context("CircuitSynthesizerConfig::from_env()")?;
    let pool = ConnectionPool::builder(DbVariant::Prover)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let vk_commitments = get_cached_commitments();

    let circuit_synthesizer = CircuitSynthesizer::new(
        config.clone(),
        ProverGroupConfig::from_env().context("ProverGroupConfig::from_env()")?,
        &ObjectStoreFactory::from_env().context("ObjectStoreFactory::from_env()")?,
        vk_commitments,
        pool,
    )
    .await
    .map_err(|err| anyhow::anyhow!("Could not initialize synthesizer {err:?}"))?;

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    tracing::info!("Starting circuit synthesizer");
    let prometheus_task =
        PrometheusExporterConfig::pull(config.prometheus_listener_port).run(stop_receiver.clone());
    let tasks = vec![
        tokio::spawn(prometheus_task),
        tokio::spawn(circuit_synthesizer.run(stop_receiver, opt.number_of_iterations)),
    ];

    let particular_crypto_alerts = Some(
        AlertsConfig::from_env()
            .context("AlertsConfig::from_env()")?
            .sporadic_crypto_errors_substrs,
    );
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        }
    };
    stop_sender.send(true).ok();
    Ok(())
}

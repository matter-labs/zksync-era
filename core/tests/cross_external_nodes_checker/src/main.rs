extern crate core;

mod checker;
mod config;
mod divergence;
mod helpers;
mod pubsub_checker;

use crate::config::CheckerConfig;
use crate::helpers::setup_sigint_handler;
use checker::Checker;
use pubsub_checker::PubSubChecker;
use tokio::sync::watch;
use zksync_utils::wait_for_tasks::wait_for_tasks;

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
            .expect("Invalid Sentry URL")
            .with_sentry_environment(environment);
    }
    let _guard = builder.build();

    tracing::info!("Started the Cross Node Checker");

    let config = CheckerConfig::from_env();
    tracing::info!("Loaded the checker config: {:?}", config);

    let mut join_handles = Vec::new();
    let sigint_receiver = setup_sigint_handler();
    let (stop_sender, stop_receiver) = watch::channel::<bool>(false);

    if config.mode.run_rpc() {
        let cross_node_checker = Checker::new(&config);
        let checker_stop_receiver = stop_receiver.clone();
        let checker_handle =
            tokio::spawn(async move { cross_node_checker.run(checker_stop_receiver).await });
        join_handles.push(checker_handle);
    }

    if config.mode.run_pubsub() {
        let pubsub_checker = PubSubChecker::new(config).await;
        let pubsub_stop_receiver = stop_receiver.clone();
        let pubsub_handle =
            tokio::spawn(async move { pubsub_checker.run(pubsub_stop_receiver).await });
        join_handles.push(pubsub_handle);
    }

    tokio::select! {
        _ = wait_for_tasks(join_handles, None, None::<futures::future::Ready<()>>, false) => {},
        _ = sigint_receiver => {
            let _ = stop_sender.send(true);
            tracing::info!("Stop signal received, shutting down the cross EN Checker");
        },
    }

    Ok(())
}

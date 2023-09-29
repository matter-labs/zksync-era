use clap::Parser;

use std::{env, str::FromStr, time::Duration};
use zksync_config::configs::chain::NetworkConfig;

use zksync_config::{ContractsConfig, ETHSenderConfig};
use zksync_core::{
    genesis_init, initialize_components, is_genesis_needed, setup_sigint_handler, Component,
    Components,
};
use zksync_storage::RocksDB;
use zksync_utils::wait_for_tasks::wait_for_tasks;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Debug, Parser)]
#[structopt(author = "Matter Labs", version, about = "zkSync operator node", long_about = None)]
struct Cli {
    /// Generate genesis block for the first contract deployment using temporary DB.
    #[arg(long)]
    genesis: bool,
    /// Rebuild tree.
    #[arg(long)]
    rebuild_tree: bool,
    /// Comma-separated list of components to launch.
    #[arg(
        long,
        default_value = "api,tree,eth,data_fetcher,state_keeper,witness_generator,housekeeper"
    )]
    components: ComponentsToRun,
}

#[derive(Debug, Clone)]
struct ComponentsToRun(Vec<Component>);

impl FromStr for ComponentsToRun {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let components = s.split(',').try_fold(vec![], |mut acc, component_str| {
            let components = Components::from_str(component_str.trim())?;
            acc.extend(components.0);
            Ok::<_, String>(acc)
        })?;
        Ok(Self(components))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();
    vlog::init();
    let sentry_guard = vlog::init_sentry();

    if opt.genesis || is_genesis_needed().await {
        let network = NetworkConfig::from_env();
        let eth_sender = ETHSenderConfig::from_env();
        let contracts = ContractsConfig::from_env();
        genesis_init(&eth_sender, &network, &contracts).await;
        if opt.genesis {
            return Ok(());
        }
    }

    if sentry_guard.is_some() {
        vlog::info!(
            "Starting Sentry url: {}, l1_network: {}, l2_network {}",
            env::var("MISC_SENTRY_URL").unwrap(),
            env::var("CHAIN_ETH_NETWORK").unwrap(),
            env::var("CHAIN_ETH_ZKSYNC_NETWORK").unwrap(),
        );
    } else {
        vlog::info!("No sentry url configured");
    }

    let components = if opt.rebuild_tree {
        vec![Component::Tree]
    } else {
        opt.components.0
    };

    // OneShotWitnessGenerator is the only component that is not expected to run indefinitely
    // if this value is `false`, we expect all components to run indefinitely: we panic if any component returns.
    let is_only_oneshot_witness_generator_task = matches!(
        components.as_slice(),
        [Component::WitnessGenerator(Some(_), _)]
    );

    // Run core actors.
    let (core_task_handles, stop_sender, cb_receiver, health_check_handle) =
        initialize_components(components, is_only_oneshot_witness_generator_task)
            .await
            .expect("Unable to start Core actors");

    vlog::info!("Running {} core task handlers", core_task_handles.len());
    let sigint_receiver = setup_sigint_handler();

    let particular_crypto_alerts = None::<Vec<String>>;
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = is_only_oneshot_witness_generator_task;
    tokio::select! {
        _ = wait_for_tasks(core_task_handles, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = sigint_receiver => {
            vlog::info!("Stop signal received, shutting down");
        },
        error = cb_receiver => {
            if let Ok(error_msg) = error {
                vlog::warn!("Circuit breaker received, shutting down. Reason: {}", error_msg);
            }
        },
    }

    stop_sender.send(true).ok();
    tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
        .await
        .unwrap();
    // Sleep for some time to let some components gracefully stop.
    tokio::time::sleep(Duration::from_secs(5)).await;
    health_check_handle.stop().await;
    vlog::info!("Stopped");
    Ok(())
}

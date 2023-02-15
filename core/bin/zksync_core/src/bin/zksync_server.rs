use std::cell::RefCell;

use futures::{channel::mpsc, executor::block_on, SinkExt, StreamExt};
use structopt::StructOpt;

use zksync_config::ZkSyncConfig;
use zksync_core::{genesis_init, initialize_components, wait_for_tasks, Component, Components};
use zksync_storage::RocksDB;

#[derive(Debug, Clone, Copy)]
pub enum ServerCommand {
    Genesis,
    Launch,
}

#[derive(StructOpt)]
#[structopt(name = "zkSync operator node", author = "Matter Labs")]
struct Opt {
    /// Generate genesis block for the first contract deployment using temporary db
    #[structopt(long)]
    genesis: bool,

    /// Rebuild tree
    #[structopt(long)]
    rebuild_tree: bool,

    /// comma-separated list of components to launch
    #[structopt(
        long,
        default_value = "api,tree,tree_lightweight,eth,data_fetcher,state_keeper,witness_generator"
    )]
    components: ComponentsToRun,
}

struct ComponentsToRun(Vec<Component>);

impl std::str::FromStr for ComponentsToRun {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let components = s
            .split(',')
            .map(|x| Components::from_str(x.trim()))
            .collect::<Result<Vec<Components>, String>>()?;
        let components = components
            .into_iter()
            .flat_map(|c| c.0)
            .collect::<Vec<Component>>();
        Ok(ComponentsToRun(components))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let mut config = ZkSyncConfig::from_env();
    let sentry_guard = vlog::init();

    if opt.genesis {
        genesis_init(config).await;
        return Ok(());
    }

    match sentry_guard {
        Some(_) => vlog::info!(
            "Starting Sentry url: {}, l1_network: {}, l2_network {}",
            std::env::var("MISC_SENTRY_URL").unwrap(),
            std::env::var("CHAIN_ETH_NETWORK").unwrap(),
            std::env::var("CHAIN_ETH_ZKSYNC_NETWORK").unwrap(),
        ),
        None => vlog::info!("No sentry url configured"),
    }

    let components = if opt.rebuild_tree {
        vec![Component::Tree]
    } else {
        opt.components.0
    };

    if cfg!(feature = "openzeppelin_tests") {
        // Set very small block timeout for tests to work faster.
        config.chain.state_keeper.block_commit_deadline_ms = 1;
    }

    genesis_init(config.clone()).await;

    // OneShotWitnessGenerator is the only component that is not expected to run indefinitely
    // if this value is `false`, we expect all components to run indefinitely: we panic if any component returns.
    let is_only_an_oneshotwitness_generator_task = components.len() == 1
        && components
            .iter()
            .all(|c| matches!(c, Component::WitnessGenerator(Some(_))));

    // Run core actors.
    let (core_task_handles, stop_sender, cb_receiver) = initialize_components(
        &config,
        components,
        is_only_an_oneshotwitness_generator_task,
    )
    .await
    .expect("Unable to start Core actors");

    vlog::info!("Running {} core task handlers", core_task_handles.len());
    let (stop_signal_sender, mut stop_signal_receiver) = mpsc::channel(256);
    {
        let stop_signal_sender = RefCell::new(stop_signal_sender.clone());
        ctrlc::set_handler(move || {
            let mut sender = stop_signal_sender.borrow_mut();
            block_on(sender.send(true)).expect("Ctrl+C signal send");
        })
        .expect("Error setting Ctrl+C handler");
    }

    tokio::select! {
        _ = async { wait_for_tasks(core_task_handles, is_only_an_oneshotwitness_generator_task).await } => {},
        _ = async { stop_signal_receiver.next().await } => {
            vlog::info!("Stop signal received, shutting down");
        },
        error = async { cb_receiver.await } => {
            if let Ok(error_msg) = error {
                vlog::warn!("Circuit breaker received, shutting down. Reason: {}", error_msg);
            }
        },
    };
    let _ = stop_sender.send(true);
    RocksDB::await_rocksdb_termination();
    // Sleep for some time to let some components gracefully stop.
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    Ok(())
}

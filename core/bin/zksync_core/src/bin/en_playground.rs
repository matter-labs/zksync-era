//! This file is a playground binary for the External Node development.
//! It's temporary and once a PoC is ready, this file will be replaced by the real EN entrypoint.
use zksync_config::ZkSyncConfig;
use zksync_core::{
    state_keeper::{seal_criteria::SealManager, ZkSyncStateKeeper},
    sync_layer::{
        batch_status_updater::run_batch_status_updater, external_io::ExternalIO,
        fetcher::MainNodeFetcher, genesis::perform_genesis_if_needed,
        mock_batch_executor::MockBatchExecutorBuilder, ActionQueue, ExternalNodeSealer,
    },
};
use zksync_dal::ConnectionPool;
use zksync_types::{Address, L1BatchNumber, MiniblockNumber};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _sentry_guard = vlog::init();
    let connection_pool = ConnectionPool::new(None, true);
    let config = ZkSyncConfig::from_env();

    vlog::info!("Started the EN playground");

    perform_genesis_if_needed(&mut connection_pool.access_storage().await, &config).await;

    let action_queue = ActionQueue::new();
    let en_sealer = ExternalNodeSealer::new(action_queue.clone());
    let sealer = SealManager::custom(
        config.chain.state_keeper.clone(),
        Vec::new(),
        en_sealer.clone().into_unconditional_batch_seal_criterion(),
        en_sealer.clone().into_miniblock_seal_criterion(),
    );

    let mock_batch_executor_base = Box::new(MockBatchExecutorBuilder);

    let io = Box::new(ExternalIO::new(Address::default(), action_queue.clone()));
    let (_stop_sender, stop_receiver) = tokio::sync::watch::channel::<bool>(false);

    let state_keeper = ZkSyncStateKeeper::new(stop_receiver, io, mock_batch_executor_base, sealer);

    // Different envs for the ease of local testing.
    // Localhost
    // let main_node_url = std::env::var("API_WEB3_JSON_RPC_HTTP_URL").unwrap();
    // Stage
    // let main_node_url = "https://z2-dev-api.zksync.dev:443";
    // Testnet
    // let main_node_url = "https://zksync2-testnet.zksync.dev:443";
    // Mainnet (doesn't work yet)
    // let main_node_url = "https://zksync2-mainnet.zksync.io:443";

    let fetcher = MainNodeFetcher::new(
        &config.api.web3_json_rpc.main_node_url.unwrap(),
        L1BatchNumber(0),
        MiniblockNumber(1),
        L1BatchNumber(0),
        L1BatchNumber(0),
        L1BatchNumber(0),
        action_queue.clone(),
    );

    let _updater_handle = std::thread::spawn(move || run_batch_status_updater(action_queue));
    let _sk_handle = tokio::task::spawn_blocking(|| state_keeper.run());
    fetcher.run().await;

    Ok(())
}

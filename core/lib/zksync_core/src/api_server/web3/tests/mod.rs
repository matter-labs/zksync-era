use std::{collections::HashMap, sync::Arc, time::Instant};

use assert_matches::assert_matches;
use async_trait::async_trait;
use jsonrpsee::core::ClientError;
use multivm::interface::ExecutionResult;
use tokio::sync::watch;
use zksync_config::configs::{
    api::Web3JsonRpcConfig,
    chain::{NetworkConfig, StateKeeperConfig},
    ContractsConfig,
};
use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool, StorageProcessor};
use zksync_health_check::CheckHealth;
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    api,
    block::{BlockGasCount, MiniblockHeader},
    fee::TransactionExecutionMetrics,
    get_intrinsic_constants,
    storage::get_code_key,
    transaction_request::CallRequest,
    tx::{
        tx_execution_info::TxExecutionStatus, ExecutionMetrics, IncludedTxLocation,
        TransactionExecutionResult,
    },
    utils::storage_key_for_eth_balance,
    AccountTreeId, Address, L1BatchNumber, L2ChainId, PackedEthSignature, StorageKey, StorageLog,
    VmEvent, H256, U256, U64,
};
use zksync_utils::u256_to_h256;
use zksync_web3_decl::{
    jsonrpsee::{core::ClientError as RpcError, http_client::HttpClient, types::error::ErrorCode},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
    types::FilterChanges,
};

use super::{metrics::ApiTransportLabel, *};
use crate::{
    api_server::{execution_sandbox::testonly::MockTransactionExecutor, tx_sender::TxSenderConfig},
    genesis::{ensure_genesis_state, GenesisParams},
    l1_gas_price::L1GasPriceProvider,
    utils::testonly::{
        create_l1_batch, create_l1_batch_metadata, create_l2_transaction, create_miniblock,
        prepare_empty_recovery_snapshot, prepare_recovery_snapshot,
    },
};

// FIXME: split off filter-related and VM-related tests
mod debug;
mod snapshots;
mod ws;

const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Mock [`L1GasPriceProvider`] that returns a constant value.
#[derive(Debug)]
struct MockL1GasPriceProvider(u64);

impl L1GasPriceProvider for MockL1GasPriceProvider {
    fn estimate_effective_gas_price(&self) -> u64 {
        self.0
    }
}

impl ApiServerHandles {
    /// Waits until the server health check reports the ready state.
    pub(crate) async fn wait_until_ready(&self) {
        let started_at = Instant::now();
        loop {
            assert!(
                started_at.elapsed() <= TEST_TIMEOUT,
                "Timed out waiting for API server"
            );
            let health = self.health_check.check_health().await;
            if health.status().is_ready() {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    pub(crate) async fn shutdown(self) {
        let stop_server = async {
            for task in self.tasks {
                match task.await {
                    Ok(Ok(())) => { /* Task successfully completed */ }
                    Err(err) if err.is_cancelled() => {
                        // Task was cancelled since the server runtime which runs the task was dropped.
                        // This is fine.
                    }
                    Err(err) => panic!("Server task panicked: {err:?}"),
                    Ok(Err(err)) => panic!("Server task failed: {err:?}"),
                }
            }
        };
        tokio::time::timeout(TEST_TIMEOUT, stop_server)
            .await
            .expect(format!("panicking at {}", chrono::Utc::now()).as_str());
    }
}

pub(crate) async fn spawn_http_server(
    network_config: &NetworkConfig,
    pool: ConnectionPool,
    tx_executor: MockTransactionExecutor,
    stop_receiver: watch::Receiver<bool>,
) -> ApiServerHandles {
    spawn_server(
        ApiTransportLabel::Http,
        network_config,
        pool,
        None,
        tx_executor,
        stop_receiver,
    )
    .await
    .0
}

async fn spawn_ws_server(
    network_config: &NetworkConfig,
    pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
    websocket_requests_per_minute_limit: Option<NonZeroU32>,
) -> (ApiServerHandles, mpsc::UnboundedReceiver<PubSubEvent>) {
    spawn_server(
        ApiTransportLabel::Ws,
        network_config,
        pool,
        websocket_requests_per_minute_limit,
        MockTransactionExecutor::default(),
        stop_receiver,
    )
    .await
}

async fn spawn_server(
    transport: ApiTransportLabel,
    network_config: &NetworkConfig,
    pool: ConnectionPool,
    websocket_requests_per_minute_limit: Option<NonZeroU32>,
    tx_executor: MockTransactionExecutor,
    stop_receiver: watch::Receiver<bool>,
) -> (ApiServerHandles, mpsc::UnboundedReceiver<PubSubEvent>) {
    let contracts_config = ContractsConfig::for_tests();
    let web3_config = Web3JsonRpcConfig::for_tests();
    let state_keeper_config = StateKeeperConfig::for_tests();
    let api_config = InternalApiConfig::new(network_config, &web3_config, &contracts_config);
    let tx_sender_config =
        TxSenderConfig::new(&state_keeper_config, &web3_config, api_config.l2_chain_id);

    let mut storage_caches = PostgresStorageCaches::new(1, 1);
    let cache_update_task = storage_caches.configure_storage_values_cache(
        1,
        pool.clone(),
        tokio::runtime::Handle::current(),
    );
    tokio::task::spawn_blocking(cache_update_task);

    let gas_adjuster = Arc::new(MockL1GasPriceProvider(1));
    let (mut tx_sender, vm_barrier) = crate::build_tx_sender(
        &tx_sender_config,
        &web3_config,
        &state_keeper_config,
        pool.clone(),
        pool.clone(),
        gas_adjuster,
        storage_caches,
    )
    .await;
    let (pub_sub_events_sender, pub_sub_events_receiver) = mpsc::unbounded_channel();

    Arc::get_mut(&mut tx_sender.0).unwrap().executor = tx_executor.into();

    let mut namespaces = Namespace::DEFAULT.to_vec();
    namespaces.extend([Namespace::Debug, Namespace::Snapshots]);

    let server_builder = match transport {
        ApiTransportLabel::Http => ApiBuilder::jsonrpsee_backend(api_config, pool).http(0),
        ApiTransportLabel::Ws => {
            let mut builder = ApiBuilder::jsonrpsee_backend(api_config, pool)
                .ws(0)
                .with_subscriptions_limit(100);
            if let Some(websocket_requests_per_minute_limit) = websocket_requests_per_minute_limit {
                builder = builder
                    .with_websocket_requests_per_minute_limit(websocket_requests_per_minute_limit);
            }
            builder
        }
    };
    let server_handles = server_builder
        .with_threads(1)
        .with_polling_interval(POLL_INTERVAL)
        .with_tx_sender(tx_sender, vm_barrier)
        .with_pub_sub_events(pub_sub_events_sender)
        .enable_api_namespaces(namespaces)
        .build(stop_receiver)
        .await
        .expect("Failed spawning JSON-RPC server");
    (server_handles, pub_sub_events_receiver)
}

#[async_trait]
trait HttpTest: Send + Sync {
    /// Prepares the storage before the server is started. The default implementation performs genesis.
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::Genesis
    }

    fn transaction_executor(&self) -> MockTransactionExecutor {
        MockTransactionExecutor::default()
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()>;
}

/// Storage initialization strategy.
#[derive(Debug)]
enum StorageInitialization {
    Genesis,
    Recovery {
        logs: Vec<StorageLog>,
        factory_deps: HashMap<H256, Vec<u8>>,
    },
}

impl StorageInitialization {
    const SNAPSHOT_RECOVERY_BLOCK: u32 = 23;

    fn empty_recovery() -> Self {
        Self::Recovery {
            logs: vec![],
            factory_deps: HashMap::new(),
        }
    }

    async fn prepare_storage(
        &self,
        network_config: &NetworkConfig,
        storage: &mut StorageProcessor<'_>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Genesis => {
                if storage.blocks_dal().is_genesis_needed().await? {
                    ensure_genesis_state(
                        storage,
                        network_config.zksync_network_id,
                        &GenesisParams::mock(),
                    )
                    .await?;
                }
            }
            Self::Recovery { logs, factory_deps } if logs.is_empty() && factory_deps.is_empty() => {
                prepare_empty_recovery_snapshot(storage, Self::SNAPSHOT_RECOVERY_BLOCK).await;
            }
            Self::Recovery { logs, factory_deps } => {
                prepare_recovery_snapshot(storage, Self::SNAPSHOT_RECOVERY_BLOCK, logs).await;
                storage
                    .storage_dal()
                    .insert_factory_deps(
                        MiniblockNumber(Self::SNAPSHOT_RECOVERY_BLOCK),
                        factory_deps,
                    )
                    .await;
            }
        }
        Ok(())
    }
}

async fn test_http_server(test: impl HttpTest) {
    let pool = ConnectionPool::test_pool().await;
    let network_config = NetworkConfig::for_tests();
    let mut storage = pool.access_storage().await.unwrap();
    test.storage_initialization()
        .prepare_storage(&network_config, &mut storage)
        .await
        .expect("Failed preparing storage for test");
    drop(storage);

    let (stop_sender, stop_receiver) = watch::channel(false);
    let server_handles = spawn_http_server(
        &network_config,
        pool.clone(),
        test.transaction_executor(),
        stop_receiver,
    )
    .await;
    server_handles.wait_until_ready().await;

    let client = <HttpClient>::builder()
        .build(format!("http://{}/", server_handles.local_addr))
        .unwrap();
    test.test(&client, &pool).await.unwrap();

    stop_sender.send_replace(true);
    server_handles.shutdown().await;
}

fn assert_logs_match(actual_logs: &[api::Log], expected_logs: &[&VmEvent]) {
    assert_eq!(actual_logs.len(), expected_logs.len());
    for (actual_log, &expected_log) in actual_logs.iter().zip(expected_logs) {
        assert_eq!(actual_log.address, expected_log.address);
        assert_eq!(actual_log.topics, expected_log.indexed_topics);
        assert_eq!(actual_log.data.0, expected_log.value);
    }
}

fn execute_l2_transaction() -> TransactionExecutionResult {
    let transaction = create_l2_transaction(1, 2);
    TransactionExecutionResult {
        hash: transaction.hash(),
        transaction: transaction.into(),
        execution_info: ExecutionMetrics::default(),
        execution_status: TxExecutionStatus::Success,
        refunded_gas: 0,
        operator_suggested_refund: 0,
        compressed_bytecodes: vec![],
        call_traces: vec![],
        revert_reason: None,
    }
}

/// Stores miniblock #1 with a single transaction and returns the miniblock header + transaction hash.
async fn store_miniblock(
    storage: &mut StorageProcessor<'_>,
    number: MiniblockNumber,
    transaction_results: &[TransactionExecutionResult],
) -> anyhow::Result<MiniblockHeader> {
    for result in transaction_results {
        let l2_tx = result.transaction.clone().try_into().unwrap();
        let tx_submission_result = storage
            .transactions_dal()
            .insert_transaction_l2(l2_tx, TransactionExecutionMetrics::default())
            .await;
        assert_matches!(tx_submission_result, L2TxSubmissionResult::Added);
    }

    let new_miniblock = create_miniblock(number.0);
    storage
        .blocks_dal()
        .insert_miniblock(&new_miniblock)
        .await?;
    storage
        .transactions_dal()
        .mark_txs_as_executed_in_miniblock(new_miniblock.number, transaction_results, 1.into())
        .await;
    Ok(new_miniblock)
}

async fn seal_l1_batch(
    storage: &mut StorageProcessor<'_>,
    number: L1BatchNumber,
) -> anyhow::Result<()> {
    let header = create_l1_batch(number.0);
    storage
        .blocks_dal()
        .insert_l1_batch(&header, &[], BlockGasCount::default(), &[], &[], 0)
        .await?;
    storage
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(number)
        .await?;
    let metadata = create_l1_batch_metadata(number.0);
    storage
        .blocks_dal()
        .save_l1_batch_metadata(number, &metadata, H256::zero(), false)
        .await?;
    Ok(())
}

async fn store_events(
    storage: &mut StorageProcessor<'_>,
    miniblock_number: u32,
    start_idx: u32,
) -> anyhow::Result<(IncludedTxLocation, Vec<VmEvent>)> {
    let new_miniblock = create_miniblock(miniblock_number);
    let l1_batch_number = L1BatchNumber(miniblock_number);
    storage
        .blocks_dal()
        .insert_miniblock(&new_miniblock)
        .await?;
    let tx_location = IncludedTxLocation {
        tx_hash: H256::repeat_byte(1),
        tx_index_in_miniblock: 0,
        tx_initiator_address: Address::repeat_byte(2),
    };
    let events = vec![
        // Matches address, doesn't match topics
        VmEvent {
            location: (l1_batch_number, start_idx),
            address: Address::repeat_byte(23),
            indexed_topics: vec![],
            value: start_idx.to_le_bytes().to_vec(),
        },
        // Doesn't match address, matches topics
        VmEvent {
            location: (l1_batch_number, start_idx + 1),
            address: Address::zero(),
            indexed_topics: vec![H256::repeat_byte(42)],
            value: (start_idx + 1).to_le_bytes().to_vec(),
        },
        // Doesn't match address or topics
        VmEvent {
            location: (l1_batch_number, start_idx + 2),
            address: Address::zero(),
            indexed_topics: vec![H256::repeat_byte(1), H256::repeat_byte(42)],
            value: (start_idx + 2).to_le_bytes().to_vec(),
        },
        // Matches both address and topics
        VmEvent {
            location: (l1_batch_number, start_idx + 3),
            address: Address::repeat_byte(23),
            indexed_topics: vec![H256::repeat_byte(42), H256::repeat_byte(111)],
            value: (start_idx + 3).to_le_bytes().to_vec(),
        },
    ];
    storage
        .events_dal()
        .save_events(
            MiniblockNumber(miniblock_number),
            &[(tx_location, events.iter().collect())],
        )
        .await;
    Ok((tx_location, events))
}

#[derive(Debug)]
struct HttpServerBasicsTest;

#[async_trait]
impl HttpTest for HttpServerBasicsTest {
    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool) -> anyhow::Result<()> {
        let block_number = client.get_block_number().await?;
        assert_eq!(block_number, U64::from(0));

        let l1_batch_number = client.get_l1_batch_number().await?;
        assert_eq!(l1_batch_number, U64::from(0));

        let genesis_l1_batch = client
            .get_l1_batch_details(L1BatchNumber(0))
            .await?
            .context("No genesis L1 batch")?;
        assert!(genesis_l1_batch.base.root_hash.is_some());
        Ok(())
    }
}

#[tokio::test]
async fn http_server_basics() {
    test_http_server(HttpServerBasicsTest).await;
}

#[derive(Debug)]
struct BlockMethodsWithSnapshotRecovery;

#[async_trait]
impl HttpTest for BlockMethodsWithSnapshotRecovery {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::empty_recovery()
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let error = client.get_block_number().await.unwrap_err();
        if let ClientError::Call(error) = error {
            assert_eq!(error.code(), ErrorCode::InvalidParams.code());
        } else {
            panic!("Unexpected error: {error:?}");
        }

        let block = client
            .get_block_by_number(api::BlockNumber::Latest, false)
            .await?;
        assert!(block.is_none());
        let block = client.get_block_by_number(1_000.into(), false).await?;
        assert!(block.is_none());

        let mut storage = pool.access_storage().await?;
        store_miniblock(&mut storage, MiniblockNumber(24), &[]).await?;
        drop(storage);

        let block_number = client.get_block_number().await?;
        let expected_block_number = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        assert_eq!(block_number, expected_block_number.into());

        for block_number in [api::BlockNumber::Latest, expected_block_number.into()] {
            let block = client
                .get_block_by_number(block_number, false)
                .await?
                .context("no latest block")?;
            assert_eq!(block.number, expected_block_number.into());
        }

        for number in [0, 1, expected_block_number - 1] {
            let error = client
                .get_block_details(MiniblockNumber(number))
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, expected_block_number);
            let error = client
                .get_raw_block_transactions(MiniblockNumber(number))
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, expected_block_number);

            let error = client
                .get_block_transaction_count_by_number(number.into())
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, expected_block_number);
            let error = client
                .get_block_by_number(number.into(), false)
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, expected_block_number);
        }

        Ok(())
    }
}

fn assert_pruned_block_error(error: &ClientError, first_retained_block: u32) {
    if let ClientError::Call(error) = error {
        assert_eq!(error.code(), ErrorCode::InvalidParams.code());
        assert!(
            error
                .message()
                .contains(&format!("first retained block is {first_retained_block}")),
            "{error:?}"
        );
        assert!(error.data().is_none(), "{error:?}");
    } else {
        panic!("Unexpected error: {error:?}");
    }
}

#[tokio::test]
async fn block_methods_with_snapshot_recovery() {
    test_http_server(BlockMethodsWithSnapshotRecovery).await;
}

#[derive(Debug)]
struct L1BatchMethodsWithSnapshotRecovery;

#[async_trait]
impl HttpTest for L1BatchMethodsWithSnapshotRecovery {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::empty_recovery()
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let error = client.get_l1_batch_number().await.unwrap_err();
        if let ClientError::Call(error) = error {
            assert_eq!(error.code(), ErrorCode::InvalidParams.code());
        } else {
            panic!("Unexpected error: {error:?}");
        }

        let mut storage = pool.access_storage().await?;
        let miniblock_number = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        store_miniblock(&mut storage, MiniblockNumber(miniblock_number), &[]).await?;
        seal_l1_batch(&mut storage, L1BatchNumber(miniblock_number)).await?;
        drop(storage);

        let l1_batch_number = client.get_l1_batch_number().await?;
        assert_eq!(l1_batch_number, miniblock_number.into());

        Ok(())
    }
}

#[tokio::test]
async fn l1_batch_methods_with_snapshot_recovery() {
    test_http_server(L1BatchMethodsWithSnapshotRecovery).await;
}

#[derive(Debug)]
struct StorageAccessWithSnapshotRecovery;

#[async_trait]
impl HttpTest for StorageAccessWithSnapshotRecovery {
    fn storage_initialization(&self) -> StorageInitialization {
        let address = Address::repeat_byte(1);
        let code_key = get_code_key(&address);
        let code_hash = H256::repeat_byte(2);
        let balance_key = storage_key_for_eth_balance(&address);
        let logs = vec![
            StorageLog::new_write_log(code_key, code_hash),
            StorageLog::new_write_log(balance_key, H256::from_low_u64_be(123)),
            StorageLog::new_write_log(
                StorageKey::new(AccountTreeId::new(address), H256::zero()),
                H256::repeat_byte(0xff),
            ),
        ];
        let factory_deps = [(code_hash, b"code".to_vec())].into();
        StorageInitialization::Recovery { logs, factory_deps }
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let mut storage = pool.access_storage().await?;

        let address = Address::repeat_byte(1);
        let first_local_miniblock = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        for number in [0, 1, first_local_miniblock - 1] {
            let number = api::BlockIdVariant::BlockNumber(number.into());
            let error = client.get_code(address, Some(number)).await.unwrap_err();
            assert_pruned_block_error(&error, first_local_miniblock);
            let error = client.get_balance(address, Some(number)).await.unwrap_err();
            assert_pruned_block_error(&error, first_local_miniblock);
            let error = client
                .get_storage_at(address, 0.into(), Some(number))
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, 24);
        }

        store_miniblock(&mut storage, MiniblockNumber(first_local_miniblock), &[]).await?;
        drop(storage);

        for number in [api::BlockNumber::Latest, first_local_miniblock.into()] {
            let number = api::BlockIdVariant::BlockNumber(number);
            let code = client.get_code(address, Some(number)).await?;
            assert_eq!(code.0, b"code");
            let balance = client.get_balance(address, Some(number)).await?;
            assert_eq!(balance, 123.into());
            let storage_value = client
                .get_storage_at(address, 0.into(), Some(number))
                .await?;
            assert_eq!(storage_value, H256::repeat_byte(0xff));
        }
        Ok(())
    }
}

#[tokio::test]
async fn storage_access_with_snapshot_recovery() {
    test_http_server(StorageAccessWithSnapshotRecovery).await;
}

#[derive(Debug)]
struct BasicFilterChangesTest {
    snapshot_recovery: bool,
}

#[async_trait]
impl HttpTest for BasicFilterChangesTest {
    fn storage_initialization(&self) -> StorageInitialization {
        if self.snapshot_recovery {
            StorageInitialization::empty_recovery()
        } else {
            StorageInitialization::Genesis
        }
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let block_filter_id = client.new_block_filter().await?;
        let tx_filter_id = client.new_pending_transaction_filter().await?;
        let tx_result = execute_l2_transaction();
        let new_tx_hash = tx_result.hash;
        let new_miniblock = store_miniblock(
            &mut pool.access_storage().await?,
            MiniblockNumber(if self.snapshot_recovery {
                StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1
            } else {
                1
            }),
            &[tx_result],
        )
        .await?;

        let block_filter_changes = client.get_filter_changes(block_filter_id).await?;
        assert_matches!(
            block_filter_changes,
            FilterChanges::Hashes(hashes) if hashes == [new_miniblock.hash]
        );
        let block_filter_changes = client.get_filter_changes(block_filter_id).await?;
        assert_matches!(block_filter_changes, FilterChanges::Hashes(hashes) if hashes.is_empty());

        let tx_filter_changes = client.get_filter_changes(tx_filter_id).await?;
        assert_matches!(
            tx_filter_changes,
            FilterChanges::Hashes(hashes) if hashes == [new_tx_hash]
        );
        let tx_filter_changes = client.get_filter_changes(tx_filter_id).await?;
        assert_matches!(tx_filter_changes, FilterChanges::Hashes(hashes) if hashes.is_empty());

        // Check uninstalling the filter.
        let removed = client.uninstall_filter(block_filter_id).await?;
        assert!(removed);
        let removed = client.uninstall_filter(block_filter_id).await?;
        assert!(!removed);

        let err = client
            .get_filter_changes(block_filter_id)
            .await
            .unwrap_err();
        assert_matches!(err, RpcError::Call(err) if err.code() == ErrorCode::InvalidParams.code());
        Ok(())
    }
}

#[tokio::test]
async fn basic_filter_changes() {
    test_http_server(BasicFilterChangesTest {
        snapshot_recovery: false,
    })
    .await;
}

#[tokio::test]
async fn basic_filter_changes_after_snapshot_recovery() {
    test_http_server(BasicFilterChangesTest {
        snapshot_recovery: true,
    })
    .await;
}

#[derive(Debug)]
struct LogFilterChangesTest {
    snapshot_recovery: bool,
}

#[async_trait]
impl HttpTest for LogFilterChangesTest {
    fn storage_initialization(&self) -> StorageInitialization {
        if self.snapshot_recovery {
            StorageInitialization::empty_recovery()
        } else {
            StorageInitialization::Genesis
        }
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let all_logs_filter_id = client.new_filter(Filter::default()).await?;
        let address_filter = Filter {
            address: Some(Address::repeat_byte(23).into()),
            ..Filter::default()
        };
        let address_filter_id = client.new_filter(address_filter).await?;
        let topics_filter = Filter {
            topics: Some(vec![Some(H256::repeat_byte(42).into())]),
            ..Filter::default()
        };
        let topics_filter_id = client.new_filter(topics_filter).await?;

        let mut storage = pool.access_storage().await?;
        let first_local_miniblock = if self.snapshot_recovery {
            StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1
        } else {
            1
        };
        let (_, events) = store_events(&mut storage, first_local_miniblock, 0).await?;
        drop(storage);
        let events: Vec<_> = events.iter().collect();

        let all_logs = client.get_filter_changes(all_logs_filter_id).await?;
        let FilterChanges::Logs(all_logs) = all_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", all_logs);
        };
        assert_logs_match(&all_logs, &events);

        let address_logs = client.get_filter_changes(address_filter_id).await?;
        let FilterChanges::Logs(address_logs) = address_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", address_logs);
        };
        assert_logs_match(&address_logs, &[events[0], events[3]]);

        let topics_logs = client.get_filter_changes(topics_filter_id).await?;
        let FilterChanges::Logs(topics_logs) = topics_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", topics_logs);
        };
        assert_logs_match(&topics_logs, &[events[1], events[3]]);

        let new_all_logs = client.get_filter_changes(all_logs_filter_id).await?;
        let FilterChanges::Hashes(new_all_logs) = new_all_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", new_all_logs);
        };
        assert!(new_all_logs.is_empty());
        Ok(())
    }
}

#[tokio::test]
async fn log_filter_changes() {
    test_http_server(LogFilterChangesTest {
        snapshot_recovery: false,
    })
    .await;
}

#[tokio::test]
async fn log_filter_changes_after_snapshot_recovery() {
    test_http_server(LogFilterChangesTest {
        snapshot_recovery: true,
    })
    .await;
}

#[derive(Debug)]
struct LogFilterChangesWithBlockBoundariesTest;

#[async_trait]
impl HttpTest for LogFilterChangesWithBlockBoundariesTest {
    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let lower_bound_filter = Filter {
            from_block: Some(api::BlockNumber::Number(2.into())),
            ..Filter::default()
        };
        let lower_bound_filter_id = client.new_filter(lower_bound_filter).await?;
        let upper_bound_filter = Filter {
            to_block: Some(api::BlockNumber::Number(1.into())),
            ..Filter::default()
        };
        let upper_bound_filter_id = client.new_filter(upper_bound_filter).await?;
        let bounded_filter = Filter {
            from_block: Some(api::BlockNumber::Number(1.into())),
            to_block: Some(api::BlockNumber::Number(1.into())),
            ..Filter::default()
        };
        let bounded_filter_id = client.new_filter(bounded_filter).await?;

        let mut storage = pool.access_storage().await?;
        let (_, events) = store_events(&mut storage, 1, 0).await?;
        drop(storage);
        let events: Vec<_> = events.iter().collect();

        let lower_bound_logs = client.get_filter_changes(lower_bound_filter_id).await?;
        assert_matches!(
            lower_bound_logs,
            FilterChanges::Hashes(hashes) if hashes.is_empty()
        );
        // ^ Since `FilterChanges` is serialized w/o a tag, an empty array will be deserialized
        // as `Hashes(_)` (the first declared variant).

        let upper_bound_logs = client.get_filter_changes(upper_bound_filter_id).await?;
        let FilterChanges::Logs(upper_bound_logs) = upper_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", upper_bound_logs);
        };
        assert_logs_match(&upper_bound_logs, &events);
        let bounded_logs = client.get_filter_changes(bounded_filter_id).await?;
        let FilterChanges::Logs(bounded_logs) = bounded_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", bounded_logs);
        };
        assert_eq!(bounded_logs, upper_bound_logs);

        // Add another miniblock with events to the storage.
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = store_events(&mut storage, 2, 4).await?;
        drop(storage);
        let new_events: Vec<_> = new_events.iter().collect();

        let lower_bound_logs = client.get_filter_changes(lower_bound_filter_id).await?;
        let FilterChanges::Logs(lower_bound_logs) = lower_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", lower_bound_logs);
        };
        assert_logs_match(&lower_bound_logs, &new_events);

        let new_upper_bound_logs = client.get_filter_changes(upper_bound_filter_id).await?;
        assert_matches!(new_upper_bound_logs, FilterChanges::Hashes(hashes) if hashes.is_empty());
        let new_bounded_logs = client.get_filter_changes(bounded_filter_id).await?;
        assert_matches!(new_bounded_logs, FilterChanges::Hashes(hashes) if hashes.is_empty());

        // Add miniblock #3. It should not be picked up by the bounded and upper bound filters,
        // and should be picked up by the lower bound filter.
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = store_events(&mut storage, 3, 8).await?;
        drop(storage);
        let new_events: Vec<_> = new_events.iter().collect();

        let bounded_logs = client.get_filter_changes(bounded_filter_id).await?;
        let FilterChanges::Hashes(bounded_logs) = bounded_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", bounded_logs);
        };
        assert!(bounded_logs.is_empty());

        let upper_bound_logs = client.get_filter_changes(upper_bound_filter_id).await?;
        let FilterChanges::Hashes(upper_bound_logs) = upper_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", upper_bound_logs);
        };
        assert!(upper_bound_logs.is_empty());

        let lower_bound_logs = client.get_filter_changes(lower_bound_filter_id).await?;
        let FilterChanges::Logs(lower_bound_logs) = lower_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", lower_bound_logs);
        };
        assert_logs_match(&lower_bound_logs, &new_events);
        Ok(())
    }
}

#[tokio::test]
async fn log_filter_changes_with_block_boundaries() {
    test_http_server(LogFilterChangesWithBlockBoundariesTest).await;
}

#[derive(Debug)]
struct CallTest;

impl CallTest {
    fn call_request() -> CallRequest {
        CallRequest {
            from: Some(Address::repeat_byte(1)),
            to: Some(Address::repeat_byte(2)),
            data: Some(b"call".to_vec().into()),
            ..CallRequest::default()
        }
    }
}

#[async_trait]
impl HttpTest for CallTest {
    fn transaction_executor(&self) -> MockTransactionExecutor {
        let mut tx_executor = MockTransactionExecutor::default();
        tx_executor.insert_call_response(
            Self::call_request().data.unwrap().0,
            ExecutionResult::Success {
                output: b"output".to_vec(),
            },
        );
        tx_executor
    }

    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool) -> anyhow::Result<()> {
        let call_result = client.call(Self::call_request(), None).await?;
        assert_eq!(call_result.0, b"output");

        let valid_block_numbers = [
            api::BlockNumber::Pending,
            api::BlockNumber::Latest,
            0.into(),
        ];
        for number in valid_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let call_result = client.call(Self::call_request(), Some(number)).await?;
            assert_eq!(call_result.0, b"output");
        }

        let invalid_block_number = api::BlockNumber::from(100);
        let number = api::BlockIdVariant::BlockNumber(invalid_block_number);
        let error = client
            .call(Self::call_request(), Some(number))
            .await
            .unwrap_err();
        if let ClientError::Call(error) = error {
            assert_eq!(error.code(), ErrorCode::InvalidParams.code());
        } else {
            panic!("Unexpected error: {error:?}");
        }

        Ok(())
    }
}

#[tokio::test]
async fn call_method_basics() {
    test_http_server(CallTest).await;
}

#[derive(Debug)]
struct CallTestAfterSnapshotRecovery;

#[async_trait]
impl HttpTest for CallTestAfterSnapshotRecovery {
    fn storage_initialization(&self) -> StorageInitialization {
        StorageInitialization::empty_recovery()
    }

    fn transaction_executor(&self) -> MockTransactionExecutor {
        CallTest.transaction_executor()
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let call_result = client.call(CallTest::call_request(), None).await?;
        assert_eq!(call_result.0, b"output");
        let pending_block_number = api::BlockIdVariant::BlockNumber(api::BlockNumber::Pending);
        let call_result = client
            .call(CallTest::call_request(), Some(pending_block_number))
            .await?;
        assert_eq!(call_result.0, b"output");

        let first_local_miniblock = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        let first_miniblock_numbers = [api::BlockNumber::Latest, first_local_miniblock.into()];
        for number in first_miniblock_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let error = client
                .call(CallTest::call_request(), Some(number))
                .await
                .unwrap_err();
            if let ClientError::Call(error) = error {
                assert_eq!(error.code(), ErrorCode::InvalidParams.code());
            } else {
                panic!("Unexpected error: {error:?}");
            }
        }

        let pruned_block_numbers = [0, 1, StorageInitialization::SNAPSHOT_RECOVERY_BLOCK];
        for number in pruned_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number.into());
            let error = client
                .call(CallTest::call_request(), Some(number))
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1);
        }

        let mut storage = pool.access_storage().await?;
        store_miniblock(&mut storage, MiniblockNumber(first_local_miniblock), &[]).await?;
        drop(storage);

        for number in first_miniblock_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let call_result = client.call(CallTest::call_request(), Some(number)).await?;
            assert_eq!(call_result.0, b"output");
        }
        Ok(())
    }
}

#[tokio::test]
async fn call_method_after_snapshot_recovery() {
    test_http_server(CallTestAfterSnapshotRecovery).await;
}

#[derive(Debug)]
struct SendRawTransactionTest {
    snapshot_recovery: bool,
}

impl SendRawTransactionTest {
    fn transaction_bytes_and_hash() -> (Vec<u8>, H256) {
        let private_key = H256::repeat_byte(11);
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();

        let tx_request = api::TransactionRequest {
            chain_id: Some(L2ChainId::default().as_u64()),
            from: Some(address),
            to: Some(Address::repeat_byte(2)),
            value: 123_456.into(),
            gas: (get_intrinsic_constants().l2_tx_intrinsic_gas * 2).into(),
            gas_price: StateKeeperConfig::for_tests().fair_l2_gas_price.into(),
            input: vec![1, 2, 3, 4].into(),
            ..api::TransactionRequest::default()
        };
        let mut rlp = Default::default();
        tx_request.rlp(&mut rlp, L2ChainId::default().as_u64(), None);
        let data = rlp.out();
        let signed_message = PackedEthSignature::message_to_signed_bytes(&data);
        let signature = PackedEthSignature::sign_raw(&private_key, &signed_message).unwrap();

        let mut rlp = Default::default();
        tx_request.rlp(&mut rlp, L2ChainId::default().as_u64(), Some(&signature));
        let data = rlp.out();
        let (_, tx_hash) =
            api::TransactionRequest::from_bytes(&data, L2ChainId::default()).unwrap();
        (data.into(), tx_hash)
    }

    fn balance_storage_log() -> StorageLog {
        let private_key = H256::repeat_byte(11);
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();
        let balance_key = storage_key_for_eth_balance(&address);
        StorageLog::new_write_log(balance_key, u256_to_h256(U256::one() << 64))
    }
}

#[async_trait]
impl HttpTest for SendRawTransactionTest {
    fn storage_initialization(&self) -> StorageInitialization {
        if self.snapshot_recovery {
            // TODO: should probably initialize logs here
            StorageInitialization::empty_recovery()
        } else {
            StorageInitialization::Genesis
        }
    }

    fn transaction_executor(&self) -> MockTransactionExecutor {
        let mut tx_executor = MockTransactionExecutor::default();
        tx_executor.insert_tx_response(
            Self::transaction_bytes_and_hash().1,
            ExecutionResult::Success { output: vec![] },
        );
        tx_executor
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        // Manually set sufficient balance for the transaction account.
        let mut storage = pool.access_storage().await?;
        storage
            .storage_dal()
            .apply_storage_logs(&[(H256::zero(), vec![Self::balance_storage_log()])])
            .await;
        drop(storage);

        let (tx_bytes, tx_hash) = Self::transaction_bytes_and_hash();
        let send_result = client.send_raw_transaction(tx_bytes.into()).await?;
        assert_eq!(send_result, tx_hash);
        Ok(())
    }
}

#[tokio::test]
async fn send_raw_transaction_basics() {
    test_http_server(SendRawTransactionTest {
        snapshot_recovery: false,
    })
    .await;
}

#[tokio::test]
async fn send_raw_transaction_after_snapshot_recovery() {
    test_http_server(SendRawTransactionTest {
        snapshot_recovery: true,
    })
    .await;
}

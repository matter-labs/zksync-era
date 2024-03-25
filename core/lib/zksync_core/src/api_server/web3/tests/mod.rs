use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    slice,
    time::Instant,
};

use assert_matches::assert_matches;
use async_trait::async_trait;
use jsonrpsee::core::{client::ClientT, params::BatchRequestBuilder, ClientError};
use multivm::zk_evm_latest::ethereum_types::U256;
use tokio::sync::watch;
use zksync_config::{
    configs::{
        api::Web3JsonRpcConfig,
        chain::{NetworkConfig, StateKeeperConfig},
        ContractsConfig,
    },
    GenesisConfig,
};
use zksync_dal::{transactions_dal::L2TxSubmissionResult, Connection, ConnectionPool, CoreDal};
use zksync_health_check::CheckHealth;
use zksync_types::{
    api,
    block::MiniblockHeader,
    fee::TransactionExecutionMetrics,
    get_nonce_key,
    l2::L2Tx,
    storage::get_code_key,
    tokens::{TokenInfo, TokenMetadata},
    tx::{
        tx_execution_info::TxExecutionStatus, ExecutionMetrics, IncludedTxLocation,
        TransactionExecutionResult,
    },
    utils::{storage_key_for_eth_balance, storage_key_for_standard_token_balance},
    AccountTreeId, Address, L1BatchNumber, Nonce, StorageKey, StorageLog, VmEvent, H256, U64,
};
use zksync_utils::u256_to_h256;
use zksync_web3_decl::{
    jsonrpsee::{http_client::HttpClient, types::error::ErrorCode},
    namespaces::{EnNamespaceClient, EthNamespaceClient, ZksNamespaceClient},
};

use super::{metrics::ApiTransportLabel, *};
use crate::{
    api_server::{
        execution_sandbox::testonly::MockTransactionExecutor,
        tx_sender::tests::create_test_tx_sender,
    },
    genesis::{insert_genesis_batch, mock_genesis_config, GenesisParams},
    utils::testonly::{
        create_l1_batch, create_l1_batch_metadata, create_l2_transaction, create_miniblock,
        l1_batch_metadata_to_commitment_artifacts, prepare_recovery_snapshot,
    },
};

mod debug;
mod filters;
mod snapshots;
mod vm;
mod ws;

const TEST_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

impl ApiServerHandles {
    /// Waits until the server health check reports the ready state. Must be called once per server instance.
    pub(crate) async fn wait_until_ready(&mut self) -> SocketAddr {
        let started_at = Instant::now();
        loop {
            assert!(
                started_at.elapsed() <= TEST_TIMEOUT,
                "Timed out waiting for API server"
            );
            let health = self.health_check.check_health().await;
            if health.status().is_healthy() {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        let mut local_addr_future = Pin::new(&mut self.local_addr);
        local_addr_future
            .as_mut()
            .await
            .expect("API server panicked");
        local_addr_future.output_mut().copied().unwrap()
    }

    pub(crate) async fn shutdown(self) {
        let stop_server = async {
            for task in self.tasks {
                match task.await {
                    Ok(Ok(())) => { /* Task successfully completed */ }
                    Err(err) if err.is_cancelled() => {
                        // Task was canceled since the server runtime which runs the task was dropped.
                        // This is fine.
                    }
                    Err(err) => panic!("Server task panicked: {err:?}"),
                    Ok(Err(err)) => panic!("Server task failed: {err:?}"),
                }
            }
        };
        tokio::time::timeout(TEST_TIMEOUT, stop_server)
            .await
            .unwrap_or_else(|_| panic!("panicking at {}", chrono::Utc::now()));
    }
}

pub(crate) async fn spawn_http_server(
    api_config: InternalApiConfig,
    pool: ConnectionPool<Core>,
    tx_executor: MockTransactionExecutor,
    method_tracer: Arc<MethodTracer>,
    stop_receiver: watch::Receiver<bool>,
) -> ApiServerHandles {
    spawn_server(
        ApiTransportLabel::Http,
        api_config,
        pool,
        None,
        tx_executor,
        method_tracer,
        stop_receiver,
    )
    .await
    .0
}

async fn spawn_ws_server(
    api_config: InternalApiConfig,
    pool: ConnectionPool<Core>,
    stop_receiver: watch::Receiver<bool>,
    websocket_requests_per_minute_limit: Option<NonZeroU32>,
) -> (ApiServerHandles, mpsc::UnboundedReceiver<PubSubEvent>) {
    spawn_server(
        ApiTransportLabel::Ws,
        api_config,
        pool,
        websocket_requests_per_minute_limit,
        MockTransactionExecutor::default(),
        Arc::default(),
        stop_receiver,
    )
    .await
}

async fn spawn_server(
    transport: ApiTransportLabel,
    api_config: InternalApiConfig,
    pool: ConnectionPool<Core>,
    websocket_requests_per_minute_limit: Option<NonZeroU32>,
    tx_executor: MockTransactionExecutor,
    method_tracer: Arc<MethodTracer>,
    stop_receiver: watch::Receiver<bool>,
) -> (ApiServerHandles, mpsc::UnboundedReceiver<PubSubEvent>) {
    let (tx_sender, vm_barrier) =
        create_test_tx_sender(pool.clone(), api_config.l2_chain_id, tx_executor.into()).await;
    let (pub_sub_events_sender, pub_sub_events_receiver) = mpsc::unbounded_channel();

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
        .with_polling_interval(POLL_INTERVAL)
        .with_tx_sender(tx_sender)
        .with_vm_barrier(vm_barrier)
        .with_pub_sub_events(pub_sub_events_sender)
        .with_method_tracer(method_tracer)
        .enable_api_namespaces(namespaces)
        .build()
        .expect("Unable to build API server")
        .run(stop_receiver)
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

    fn method_tracer(&self) -> Arc<MethodTracer> {
        Arc::default()
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool<Core>) -> anyhow::Result<()>;

    /// Overrides the `filters_disabled` configuration parameter for HTTP server startup
    fn filters_disabled(&self) -> bool {
        false
    }
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
    const SNAPSHOT_RECOVERY_BATCH: L1BatchNumber = L1BatchNumber(23);
    const SNAPSHOT_RECOVERY_BLOCK: MiniblockNumber = MiniblockNumber(23);

    fn empty_recovery() -> Self {
        Self::Recovery {
            logs: vec![],
            factory_deps: HashMap::new(),
        }
    }

    async fn prepare_storage(
        &self,
        network_config: &NetworkConfig,
        storage: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Genesis => {
                let params = GenesisParams::load_genesis_params(GenesisConfig {
                    l2_chain_id: network_config.zksync_network_id,
                    ..mock_genesis_config()
                })
                .unwrap();
                if storage.blocks_dal().is_genesis_needed().await? {
                    insert_genesis_batch(storage, &params).await?;
                }
            }
            Self::Recovery { logs, factory_deps } => {
                prepare_recovery_snapshot(
                    storage,
                    Self::SNAPSHOT_RECOVERY_BATCH,
                    Self::SNAPSHOT_RECOVERY_BLOCK,
                    logs,
                )
                .await;
                storage
                    .factory_deps_dal()
                    .insert_factory_deps(Self::SNAPSHOT_RECOVERY_BLOCK, factory_deps)
                    .await?;

                // Insert the next L1 batch in the storage so that the API server doesn't hang up.
                store_miniblock(storage, Self::SNAPSHOT_RECOVERY_BLOCK + 1, &[]).await?;
                seal_l1_batch(storage, Self::SNAPSHOT_RECOVERY_BATCH + 1).await?;
            }
        }
        Ok(())
    }
}

async fn test_http_server(test: impl HttpTest) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let network_config = NetworkConfig::for_tests();
    let mut storage = pool.connection().await.unwrap();
    test.storage_initialization()
        .prepare_storage(&network_config, &mut storage)
        .await
        .expect("Failed preparing storage for test");
    drop(storage);

    let (stop_sender, stop_receiver) = watch::channel(false);
    let contracts_config = ContractsConfig::for_tests();
    let web3_config = Web3JsonRpcConfig::for_tests();
    let mut api_config = InternalApiConfig::new(&network_config, &web3_config, &contracts_config);
    api_config.filters_disabled = test.filters_disabled();
    let mut server_handles = spawn_http_server(
        api_config,
        pool.clone(),
        test.transaction_executor(),
        test.method_tracer(),
        stop_receiver,
    )
    .await;

    let local_addr = server_handles.wait_until_ready().await;
    let client = <HttpClient>::builder()
        .build(format!("http://{local_addr}/"))
        .unwrap();
    test.test(&client, &pool).await.unwrap();

    stop_sender.send_replace(true);
    server_handles.shutdown().await;
}

fn assert_logs_match(actual_logs: &[api::Log], expected_logs: &[&VmEvent]) {
    assert_eq!(
        actual_logs.len(),
        expected_logs.len(),
        "expected: {expected_logs:?}, actual: {actual_logs:?}"
    );
    for (actual_log, &expected_log) in actual_logs.iter().zip(expected_logs) {
        assert_eq!(
            actual_log.address, expected_log.address,
            "expected: {expected_logs:?}, actual: {actual_logs:?}"
        );
        assert_eq!(
            actual_log.topics, expected_log.indexed_topics,
            "expected: {expected_logs:?}, actual: {actual_logs:?}"
        );
        assert_eq!(
            actual_log.data.0, expected_log.value,
            "expected: {expected_logs:?}, actual: {actual_logs:?}"
        );
    }
}

fn execute_l2_transaction(transaction: L2Tx) -> TransactionExecutionResult {
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
    storage: &mut Connection<'_, Core>,
    number: MiniblockNumber,
    transaction_results: &[TransactionExecutionResult],
) -> anyhow::Result<MiniblockHeader> {
    for result in transaction_results {
        let l2_tx = result.transaction.clone().try_into().unwrap();
        let tx_submission_result = storage
            .transactions_dal()
            .insert_transaction_l2(l2_tx, TransactionExecutionMetrics::default())
            .await
            .unwrap();
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
    storage: &mut Connection<'_, Core>,
    number: L1BatchNumber,
) -> anyhow::Result<()> {
    let header = create_l1_batch(number.0);
    storage.blocks_dal().insert_mock_l1_batch(&header).await?;
    storage
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(number)
        .await?;
    let metadata = create_l1_batch_metadata(number.0);
    storage
        .blocks_dal()
        .save_l1_batch_tree_data(number, &metadata.tree_data())
        .await?;
    storage
        .blocks_dal()
        .save_l1_batch_commitment_artifacts(
            number,
            &l1_batch_metadata_to_commitment_artifacts(&metadata),
        )
        .await?;
    Ok(())
}

async fn store_events(
    storage: &mut Connection<'_, Core>,
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
    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
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

    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let block = client.get_block_by_number(1_000.into(), false).await?;
        assert!(block.is_none());

        let expected_block_number = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        let block_number = client.get_block_number().await?;
        assert_eq!(block_number, expected_block_number.0.into());

        for block_number in [api::BlockNumber::Latest, expected_block_number.0.into()] {
            let block = client
                .get_block_by_number(block_number, false)
                .await?
                .context("no latest block")?;
            assert_eq!(block.number, expected_block_number.0.into());
        }

        for number in [0, 1, StorageInitialization::SNAPSHOT_RECOVERY_BLOCK.0] {
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

fn assert_pruned_block_error(error: &ClientError, first_retained_block: MiniblockNumber) {
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

    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let miniblock_number = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        let l1_batch_number = StorageInitialization::SNAPSHOT_RECOVERY_BATCH + 1;
        assert_eq!(
            client.get_l1_batch_number().await?,
            l1_batch_number.0.into()
        );

        // `get_miniblock_range` method
        let miniblock_range = client
            .get_miniblock_range(l1_batch_number)
            .await?
            .context("no range for sealed L1 batch")?;
        assert_eq!(miniblock_range.0, miniblock_number.0.into());
        assert_eq!(miniblock_range.1, miniblock_number.0.into());

        let miniblock_range_for_future_batch =
            client.get_miniblock_range(l1_batch_number + 1).await?;
        assert_eq!(miniblock_range_for_future_batch, None);

        let error = client
            .get_miniblock_range(l1_batch_number - 1)
            .await
            .unwrap_err();
        assert_pruned_l1_batch_error(&error, l1_batch_number);

        // `get_l1_batch_details` method
        let details = client
            .get_l1_batch_details(l1_batch_number)
            .await?
            .context("no details for sealed L1 batch")?;
        assert_eq!(details.number, l1_batch_number);

        let details_for_future_batch = client.get_l1_batch_details(l1_batch_number + 1).await?;
        assert!(
            details_for_future_batch.is_none(),
            "{details_for_future_batch:?}"
        );

        let error = client
            .get_l1_batch_details(l1_batch_number - 1)
            .await
            .unwrap_err();
        assert_pruned_l1_batch_error(&error, l1_batch_number);

        Ok(())
    }
}

fn assert_pruned_l1_batch_error(error: &ClientError, first_retained_l1_batch: L1BatchNumber) {
    if let ClientError::Call(error) = error {
        assert_eq!(error.code(), ErrorCode::InvalidParams.code());
        assert!(
            error.message().contains(&format!(
                "first retained L1 batch is {first_retained_l1_batch}"
            )),
            "{error:?}"
        );
        assert!(error.data().is_none(), "{error:?}");
    } else {
        panic!("Unexpected error: {error:?}");
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

    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let address = Address::repeat_byte(1);
        let first_local_miniblock = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        for number in [0, 1, first_local_miniblock.0 - 1] {
            let number = api::BlockIdVariant::BlockNumber(number.into());
            let error = client.get_code(address, Some(number)).await.unwrap_err();
            assert_pruned_block_error(&error, first_local_miniblock);
            let error = client.get_balance(address, Some(number)).await.unwrap_err();
            assert_pruned_block_error(&error, first_local_miniblock);
            let error = client
                .get_storage_at(address, 0.into(), Some(number))
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, first_local_miniblock);
        }

        for number in [api::BlockNumber::Latest, first_local_miniblock.0.into()] {
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
struct TransactionCountTest;

#[async_trait]
impl HttpTest for TransactionCountTest {
    async fn test(&self, client: &HttpClient, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let test_address = Address::repeat_byte(11);
        let mut storage = pool.connection().await?;
        let mut miniblock_number = MiniblockNumber(0);
        for nonce in [0, 1] {
            let mut committed_tx = create_l2_transaction(10, 200);
            committed_tx.common_data.initiator_address = test_address;
            committed_tx.common_data.nonce = Nonce(nonce);
            miniblock_number += 1;
            store_miniblock(
                &mut storage,
                miniblock_number,
                &[execute_l2_transaction(committed_tx)],
            )
            .await?;
            let nonce_log = StorageLog::new_write_log(
                get_nonce_key(&test_address),
                H256::from_low_u64_be((nonce + 1).into()),
            );
            storage
                .storage_logs_dal()
                .insert_storage_logs(miniblock_number, &[(H256::zero(), vec![nonce_log])])
                .await?;
        }

        let pending_count = client.get_transaction_count(test_address, None).await?;
        assert_eq!(pending_count, 2.into());

        let mut pending_tx = create_l2_transaction(10, 200);
        pending_tx.common_data.initiator_address = test_address;
        pending_tx.common_data.nonce = Nonce(2);
        storage
            .transactions_dal()
            .insert_transaction_l2(pending_tx, TransactionExecutionMetrics::default())
            .await
            .unwrap();

        let pending_count = client.get_transaction_count(test_address, None).await?;
        assert_eq!(pending_count, 3.into());

        let latest_block_numbers = [api::BlockNumber::Latest, miniblock_number.0.into()];
        for number in latest_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let latest_count = client
                .get_transaction_count(test_address, Some(number))
                .await?;
            assert_eq!(latest_count, 2.into());
        }

        let earliest_block_numbers = [api::BlockNumber::Earliest, 0.into()];
        for number in earliest_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let historic_count = client
                .get_transaction_count(test_address, Some(number))
                .await?;
            assert_eq!(historic_count, 0.into());
        }

        let number = api::BlockIdVariant::BlockNumber(1.into());
        let historic_count = client
            .get_transaction_count(test_address, Some(number))
            .await?;
        assert_eq!(historic_count, 1.into());

        let number = api::BlockIdVariant::BlockNumber(100.into());
        let error = client
            .get_transaction_count(test_address, Some(number))
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
async fn getting_transaction_count_for_account() {
    test_http_server(TransactionCountTest).await;
}

#[derive(Debug)]
struct TransactionCountAfterSnapshotRecoveryTest;

#[async_trait]
impl HttpTest for TransactionCountAfterSnapshotRecoveryTest {
    fn storage_initialization(&self) -> StorageInitialization {
        let test_address = Address::repeat_byte(11);
        let nonce_log =
            StorageLog::new_write_log(get_nonce_key(&test_address), H256::from_low_u64_be(3));
        StorageInitialization::Recovery {
            logs: vec![nonce_log],
            factory_deps: HashMap::new(),
        }
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let test_address = Address::repeat_byte(11);
        let pending_count = client.get_transaction_count(test_address, None).await?;
        assert_eq!(pending_count, 3.into());

        let mut pending_tx = create_l2_transaction(10, 200);
        pending_tx.common_data.initiator_address = test_address;
        pending_tx.common_data.nonce = Nonce(3);
        let mut storage = pool.connection().await?;
        storage
            .transactions_dal()
            .insert_transaction_l2(pending_tx, TransactionExecutionMetrics::default())
            .await
            .unwrap();

        let pending_count = client.get_transaction_count(test_address, None).await?;
        assert_eq!(pending_count, 4.into());

        let pruned_block_numbers = [
            api::BlockNumber::Earliest,
            0.into(),
            StorageInitialization::SNAPSHOT_RECOVERY_BLOCK.0.into(),
        ];
        for number in pruned_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let error = client
                .get_transaction_count(test_address, Some(number))
                .await
                .unwrap_err();
            assert_pruned_block_error(&error, StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1);
        }

        let latest_miniblock_number = StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1;
        let latest_block_numbers = [api::BlockNumber::Latest, latest_miniblock_number.0.into()];
        for number in latest_block_numbers {
            let number = api::BlockIdVariant::BlockNumber(number);
            let latest_count = client
                .get_transaction_count(test_address, Some(number))
                .await?;
            assert_eq!(latest_count, 3.into());
        }
        Ok(())
    }
}

#[tokio::test]
async fn getting_transaction_count_for_account_after_snapshot_recovery() {
    test_http_server(TransactionCountAfterSnapshotRecoveryTest).await;
}

#[derive(Debug)]
struct TransactionReceiptsTest;

#[async_trait]
impl HttpTest for TransactionReceiptsTest {
    async fn test(&self, client: &HttpClient, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let mut storage = pool.connection().await?;
        let miniblock_number = MiniblockNumber(1);

        let tx1 = create_l2_transaction(10, 200);
        let tx2 = create_l2_transaction(10, 200);
        let tx_results = vec![
            execute_l2_transaction(tx1.clone()),
            execute_l2_transaction(tx2.clone()),
        ];
        store_miniblock(&mut storage, miniblock_number, &tx_results).await?;

        let mut expected_receipts = Vec::new();
        for tx in &tx_results {
            expected_receipts.push(
                client
                    .get_transaction_receipt(tx.hash)
                    .await?
                    .context("no receipt")?,
            );
        }
        for (tx_result, receipt) in tx_results.iter().zip(&expected_receipts) {
            assert_eq!(tx_result.hash, receipt.transaction_hash);
        }

        let receipts = client
            .get_block_receipts(api::BlockId::Number(miniblock_number.0.into()))
            .await?
            .context("no receipts")?;
        assert_eq!(receipts.len(), 2);
        for (receipt, expected_receipt) in receipts.iter().zip(&expected_receipts) {
            assert_eq!(receipt, expected_receipt);
        }

        // Check receipts for a missing block.
        let receipts = client
            .get_block_receipts(api::BlockId::Number(100.into()))
            .await?;
        assert!(receipts.is_none());

        Ok(())
    }
}

#[tokio::test]
async fn transaction_receipts() {
    test_http_server(TransactionReceiptsTest).await;
}

#[derive(Debug)]
struct AllAccountBalancesTest;

impl AllAccountBalancesTest {
    const ADDRESS: Address = Address::repeat_byte(0x11);
}

#[async_trait]
impl HttpTest for AllAccountBalancesTest {
    async fn test(&self, client: &HttpClient, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let balances = client.get_all_account_balances(Self::ADDRESS).await?;
        assert_eq!(balances, HashMap::new());

        let mut storage = pool.connection().await?;
        store_miniblock(&mut storage, MiniblockNumber(1), &[]).await?;

        let eth_balance_key = storage_key_for_eth_balance(&Self::ADDRESS);
        let eth_balance = U256::one() << 64;
        let eth_balance_log = StorageLog::new_write_log(eth_balance_key, u256_to_h256(eth_balance));
        storage
            .storage_logs_dal()
            .insert_storage_logs(MiniblockNumber(1), &[(H256::zero(), vec![eth_balance_log])])
            .await?;
        // Create a custom token, but don't set balance for it yet.
        let custom_token = TokenInfo {
            l1_address: Address::repeat_byte(0xfe),
            l2_address: Address::repeat_byte(0xfe),
            metadata: TokenMetadata::default(Address::repeat_byte(0xfe)),
        };
        storage
            .tokens_dal()
            .add_tokens(slice::from_ref(&custom_token))
            .await?;

        let balances = client.get_all_account_balances(Self::ADDRESS).await?;
        assert_eq!(balances, HashMap::from([(Address::zero(), eth_balance)]));

        store_miniblock(&mut storage, MiniblockNumber(2), &[]).await?;
        let token_balance_key = storage_key_for_standard_token_balance(
            AccountTreeId::new(custom_token.l2_address),
            &Self::ADDRESS,
        );
        let token_balance = 123.into();
        let token_balance_log =
            StorageLog::new_write_log(token_balance_key, u256_to_h256(token_balance));
        storage
            .storage_logs_dal()
            .insert_storage_logs(
                MiniblockNumber(2),
                &[(H256::zero(), vec![token_balance_log])],
            )
            .await?;

        let balances = client.get_all_account_balances(Self::ADDRESS).await?;
        assert_eq!(
            balances,
            HashMap::from([
                (Address::zero(), eth_balance),
                (custom_token.l2_address, token_balance),
            ])
        );
        Ok(())
    }
}

#[tokio::test]
async fn getting_all_account_balances() {
    test_http_server(AllAccountBalancesTest).await;
}

#[derive(Debug, Default)]
struct RpcCallsTracingTest {
    tracer: Arc<MethodTracer>,
}

#[async_trait]
impl HttpTest for RpcCallsTracingTest {
    fn method_tracer(&self) -> Arc<MethodTracer> {
        self.tracer.clone()
    }

    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let block_number = client.get_block_number().await?;
        assert_eq!(block_number, U64::from(0));

        let calls = self.tracer.recorded_calls().take();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].response.is_success());
        assert_eq!(calls[0].metadata.name, "eth_blockNumber");
        assert_eq!(calls[0].metadata.block_id, None);
        assert_eq!(calls[0].metadata.block_diff, None);

        client
            .get_block_by_number(api::BlockNumber::Latest, false)
            .await?;

        let calls = self.tracer.recorded_calls().take();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].response.is_success());
        assert_eq!(calls[0].metadata.name, "eth_getBlockByNumber");
        assert_eq!(
            calls[0].metadata.block_id,
            Some(api::BlockId::Number(api::BlockNumber::Latest))
        );
        assert_eq!(calls[0].metadata.block_diff, Some(0));

        let block_number = api::BlockNumber::Number(1.into());
        client.get_block_by_number(block_number, false).await?;

        let calls = self.tracer.recorded_calls().take();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].response.is_success());
        assert_eq!(calls[0].metadata.name, "eth_getBlockByNumber");
        assert_eq!(
            calls[0].metadata.block_id,
            Some(api::BlockId::Number(block_number))
        );
        assert_eq!(calls[0].metadata.block_diff, None);

        // Check protocol-level errors.
        client
            .request::<serde_json::Value, _>("eth_unknownMethod", jsonrpsee::rpc_params![])
            .await
            .unwrap_err();

        let calls = self.tracer.recorded_calls().take();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].response.as_error_code(),
            Some(ErrorCode::MethodNotFound.code())
        );
        assert!(!calls[0].metadata.has_app_error);

        client
            .request::<serde_json::Value, _>("eth_getBlockByNumber", jsonrpsee::rpc_params![0])
            .await
            .unwrap_err();

        let calls = self.tracer.recorded_calls().take();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].response.as_error_code(),
            Some(ErrorCode::InvalidParams.code())
        );
        assert!(!calls[0].metadata.has_app_error);

        // Check app-level error.
        client
            .request::<serde_json::Value, _>(
                "eth_getFilterLogs",
                jsonrpsee::rpc_params![U256::from(1)],
            )
            .await
            .unwrap_err();

        let calls = self.tracer.recorded_calls().take();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].response.as_error_code(),
            Some(ErrorCode::InvalidParams.code())
        );
        assert!(calls[0].metadata.has_app_error);

        // Check batch RPC request.
        let mut batch = BatchRequestBuilder::new();
        batch.insert("eth_blockNumber", jsonrpsee::rpc_params![])?;
        batch.insert("zks_L1BatchNumber", jsonrpsee::rpc_params![])?;
        let response = client.batch_request::<U64>(batch).await?;
        for response_part in response {
            assert_eq!(response_part.unwrap(), U64::from(0));
        }

        let calls = self.tracer.recorded_calls().take();
        assert_eq!(calls.len(), 2);
        let call_names: HashSet<_> = calls.iter().map(|call| call.metadata.name).collect();
        assert_eq!(
            call_names,
            HashSet::from(["eth_blockNumber", "zks_L1BatchNumber"])
        );

        Ok(())
    }
}

#[tokio::test]
async fn tracing_rpc_calls() {
    test_http_server(RpcCallsTracingTest::default()).await;
}

#[derive(Debug, Default)]
struct GenesisConfigTest;

#[async_trait]
impl HttpTest for GenesisConfigTest {
    async fn test(&self, client: &HttpClient, _pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        // It's enough to check that we fill all fields and deserialization is correct.
        // Mocking values is not suitable since they will always change
        client.genesis_config().await.unwrap();
        Ok(())
    }
}

#[tokio::test]
async fn tracing_genesis_config() {
    test_http_server(GenesisConfigTest).await;
}

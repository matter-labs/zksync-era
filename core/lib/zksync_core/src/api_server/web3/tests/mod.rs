use std::{collections::HashMap, time::Instant};

use assert_matches::assert_matches;
use async_trait::async_trait;
use jsonrpsee::core::ClientError;
use tokio::sync::watch;
use zksync_config::configs::{
    api::Web3JsonRpcConfig,
    chain::{NetworkConfig, StateKeeperConfig},
    ContractsConfig,
};
use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool, StorageProcessor};
use zksync_health_check::CheckHealth;
use zksync_types::{
    api,
    block::{BlockGasCount, MiniblockHeader},
    fee::TransactionExecutionMetrics,
    get_nonce_key,
    l2::L2Tx,
    storage::get_code_key,
    tx::{
        tx_execution_info::TxExecutionStatus, ExecutionMetrics, IncludedTxLocation,
        TransactionExecutionResult,
    },
    utils::storage_key_for_eth_balance,
    AccountTreeId, Address, L1BatchNumber, Nonce, StorageKey, StorageLog, VmEvent, H256, U64,
};
use zksync_web3_decl::{
    jsonrpsee::{http_client::HttpClient, types::error::ErrorCode},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

use super::{metrics::ApiTransportLabel, *};
use crate::{
    api_server::{
        execution_sandbox::testonly::MockTransactionExecutor,
        tx_sender::tests::create_test_tx_sender,
    },
    genesis::{ensure_genesis_state, GenesisParams},
    utils::testonly::{
        create_l1_batch, create_l1_batch_metadata, create_l2_transaction, create_miniblock,
        prepare_empty_recovery_snapshot, prepare_recovery_snapshot,
    },
};

mod debug;
mod filters;
mod snapshots;
mod vm;
mod ws;

const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

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
    let api_config = InternalApiConfig::new(network_config, &web3_config, &contracts_config);
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

        // `get_miniblock_range` method
        let miniblock_range = client
            .get_miniblock_range(L1BatchNumber(miniblock_number))
            .await?
            .context("no range for sealed L1 batch")?;
        assert_eq!(miniblock_range.0, miniblock_number.into());
        assert_eq!(miniblock_range.1, miniblock_number.into());

        let miniblock_range_for_future_batch = client
            .get_miniblock_range(L1BatchNumber(miniblock_number) + 1)
            .await?;
        assert_eq!(miniblock_range_for_future_batch, None);

        let error = client
            .get_miniblock_range(L1BatchNumber(miniblock_number) - 1)
            .await
            .unwrap_err();
        assert_pruned_l1_batch_error(&error, miniblock_number);

        // `get_l1_batch_details` method
        let details = client
            .get_l1_batch_details(L1BatchNumber(miniblock_number))
            .await?
            .context("no details for sealed L1 batch")?;
        assert_eq!(details.number, L1BatchNumber(miniblock_number));

        let details_for_future_batch = client
            .get_l1_batch_details(L1BatchNumber(miniblock_number) + 1)
            .await?;
        assert!(
            details_for_future_batch.is_none(),
            "{details_for_future_batch:?}"
        );

        let error = client
            .get_l1_batch_details(L1BatchNumber(miniblock_number) - 1)
            .await
            .unwrap_err();
        assert_pruned_l1_batch_error(&error, miniblock_number);

        Ok(())
    }
}

fn assert_pruned_l1_batch_error(error: &ClientError, first_retained_l1_batch: u32) {
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
struct TransactionCountTest;

#[async_trait]
impl HttpTest for TransactionCountTest {
    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let test_address = Address::repeat_byte(11);
        let mut storage = pool.access_storage().await?;
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
                .await;
        }

        let pending_count = client.get_transaction_count(test_address, None).await?;
        assert_eq!(pending_count, 2.into());

        let mut pending_tx = create_l2_transaction(10, 200);
        pending_tx.common_data.initiator_address = test_address;
        pending_tx.common_data.nonce = Nonce(2);
        storage
            .transactions_dal()
            .insert_transaction_l2(pending_tx, TransactionExecutionMetrics::default())
            .await;

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

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let test_address = Address::repeat_byte(11);
        let pending_count = client.get_transaction_count(test_address, None).await?;
        assert_eq!(pending_count, 3.into());

        let mut pending_tx = create_l2_transaction(10, 200);
        pending_tx.common_data.initiator_address = test_address;
        pending_tx.common_data.nonce = Nonce(3);
        let mut storage = pool.access_storage().await?;
        storage
            .transactions_dal()
            .insert_transaction_l2(pending_tx, TransactionExecutionMetrics::default())
            .await;

        let pending_count = client.get_transaction_count(test_address, None).await?;
        assert_eq!(pending_count, 4.into());

        let pruned_block_numbers = [
            api::BlockNumber::Earliest,
            0.into(),
            StorageInitialization::SNAPSHOT_RECOVERY_BLOCK.into(),
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
        store_miniblock(&mut storage, MiniblockNumber(latest_miniblock_number), &[]).await?;

        let latest_block_numbers = [api::BlockNumber::Latest, latest_miniblock_number.into()];
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

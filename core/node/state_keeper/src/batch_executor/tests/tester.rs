//! Testing harness for the batch executor.
//! Contains helper functionality to initialize test context and perform tests without too much boilerplate.

use std::{collections::HashMap, fmt::Debug, sync::Arc};

use multivm::{
    interface::{L1BatchEnv, L2BlockEnv, SystemEnv},
    vm_latest::constants::INITIAL_STORAGE_WRITE_PUBDATA_BYTES,
};
use tempfile::TempDir;
use tokio::{sync::watch, task::JoinHandle};
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_contracts::{get_loadnext_contract, test_contracts::LoadnextContractExecutionParams};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_node_genesis::create_genesis_l1_batch;
use zksync_node_test_utils::prepare_recovery_snapshot;
use zksync_state::{ReadStorageFactory, RocksdbStorageOptions};
use zksync_test_account::{Account, DeployContractsTx, TxType};
use zksync_types::{
    block::L2BlockHasher, ethabi::Token, fee::Fee, snapshots::SnapshotRecoveryStatus,
    storage_writes_deduplicator::StorageWritesDeduplicator,
    system_contracts::get_system_smart_contracts, utils::storage_key_for_standard_token_balance,
    AccountTreeId, Address, Execute, L1BatchNumber, L2BlockNumber, PriorityOpId, ProtocolVersionId,
    StorageKey, StorageLog, Transaction, H256, L2_BASE_TOKEN_ADDRESS,
    SYSTEM_CONTEXT_MINIMAL_BASE_FEE, U256,
};
use zksync_utils::u256_to_h256;

use super::{
    read_storage_factory::{PostgresFactory, RocksdbFactory},
    StorageType,
};
use crate::{
    batch_executor::{BatchExecutorHandle, TxExecutionResult},
    testonly::BASE_SYSTEM_CONTRACTS,
    tests::{default_l1_batch_env, default_system_env},
    AsyncRocksdbCache, BatchExecutor, MainBatchExecutor,
};

const DEFAULT_GAS_PER_PUBDATA: u32 = 10000;

/// Representation of configuration parameters used by the state keeper.
/// Has sensible defaults for most tests, each of which can be overridden.
#[derive(Debug)]
pub(super) struct TestConfig {
    pub(super) save_call_traces: bool,
    pub(super) vm_gas_limit: Option<u32>,
    pub(super) validation_computational_gas_limit: u32,
}

impl TestConfig {
    pub(super) fn new() -> Self {
        let config = StateKeeperConfig::for_tests();

        Self {
            vm_gas_limit: None,
            save_call_traces: false,
            validation_computational_gas_limit: config.validation_computational_gas_limit,
        }
    }
}

/// Tester represents an entity that can initialize the state and create batch executors over this storage.
/// Important: `Tester` must be a *sole* owner of the `ConnectionPool`, since the test pool cannot be shared.
#[derive(Debug)]
pub(super) struct Tester {
    fee_account: Address,
    db_dir: TempDir,
    pool: ConnectionPool<Core>,
    config: TestConfig,
    tasks: Vec<JoinHandle<()>>,
}

impl Tester {
    pub(super) fn new(pool: ConnectionPool<Core>) -> Self {
        Self::with_config(pool, TestConfig::new())
    }

    pub(super) fn with_config(pool: ConnectionPool<Core>, config: TestConfig) -> Self {
        Self {
            fee_account: Address::repeat_byte(0x01),
            db_dir: TempDir::new().unwrap(),
            pool,
            config,
            tasks: Vec::new(),
        }
    }

    pub(super) fn set_config(&mut self, config: TestConfig) {
        self.config = config;
    }

    /// Creates a batch executor instance with the specified storage type.
    /// This function intentionally uses sensible defaults to not introduce boilerplate.
    pub(super) async fn create_batch_executor(
        &mut self,
        storage_type: StorageType,
    ) -> BatchExecutorHandle {
        let (l1_batch_env, system_env) = self.default_batch_params();
        match storage_type {
            StorageType::AsyncRocksdbCache => {
                let (l1_batch_env, system_env) = self.default_batch_params();
                let (state_keeper_storage, task) = AsyncRocksdbCache::new(
                    self.pool(),
                    self.state_keeper_db_path(),
                    RocksdbStorageOptions::default(),
                );
                let handle = tokio::task::spawn(async move {
                    let (_stop_sender, stop_receiver) = watch::channel(false);
                    task.run(stop_receiver).await.unwrap()
                });
                self.tasks.push(handle);
                self.create_batch_executor_inner(
                    Arc::new(state_keeper_storage),
                    l1_batch_env,
                    system_env,
                )
                .await
            }
            StorageType::Rocksdb => {
                self.create_batch_executor_inner(
                    Arc::new(RocksdbFactory::new(
                        self.pool(),
                        self.state_keeper_db_path(),
                    )),
                    l1_batch_env,
                    system_env,
                )
                .await
            }
            StorageType::Postgres => {
                self.create_batch_executor_inner(
                    Arc::new(PostgresFactory::new(self.pool())),
                    l1_batch_env,
                    system_env,
                )
                .await
            }
        }
    }

    async fn create_batch_executor_inner(
        &self,
        storage_factory: Arc<dyn ReadStorageFactory>,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
    ) -> BatchExecutorHandle {
        let mut batch_executor = MainBatchExecutor::new(self.config.save_call_traces, false);
        let (_stop_sender, stop_receiver) = watch::channel(false);
        batch_executor
            .init_batch(storage_factory, l1_batch_env, system_env, &stop_receiver)
            .await
            .expect("Batch executor was interrupted")
    }

    pub(super) async fn recover_batch_executor(
        &mut self,
        snapshot: &SnapshotRecoveryStatus,
    ) -> BatchExecutorHandle {
        let (storage_factory, task) = AsyncRocksdbCache::new(
            self.pool(),
            self.state_keeper_db_path(),
            RocksdbStorageOptions::default(),
        );
        let (_, stop_receiver) = watch::channel(false);
        let handle = tokio::task::spawn(async move { task.run(stop_receiver).await.unwrap() });
        self.tasks.push(handle);
        self.recover_batch_executor_inner(Arc::new(storage_factory), snapshot)
            .await
    }

    pub(super) async fn recover_batch_executor_custom(
        &mut self,
        storage_type: &StorageType,
        snapshot: &SnapshotRecoveryStatus,
    ) -> BatchExecutorHandle {
        match storage_type {
            StorageType::AsyncRocksdbCache => self.recover_batch_executor(snapshot).await,
            StorageType::Rocksdb => {
                self.recover_batch_executor_inner(
                    Arc::new(RocksdbFactory::new(
                        self.pool(),
                        self.state_keeper_db_path(),
                    )),
                    snapshot,
                )
                .await
            }
            StorageType::Postgres => {
                self.recover_batch_executor_inner(
                    Arc::new(PostgresFactory::new(self.pool())),
                    snapshot,
                )
                .await
            }
        }
    }

    async fn recover_batch_executor_inner(
        &self,
        storage_factory: Arc<dyn ReadStorageFactory>,
        snapshot: &SnapshotRecoveryStatus,
    ) -> BatchExecutorHandle {
        let current_timestamp = snapshot.l2_block_timestamp + 1;
        let (mut l1_batch_env, system_env) =
            self.batch_params(snapshot.l1_batch_number + 1, current_timestamp);
        l1_batch_env.previous_batch_hash = Some(snapshot.l1_batch_root_hash);
        l1_batch_env.first_l2_block = L2BlockEnv {
            number: snapshot.l2_block_number.0 + 1,
            timestamp: current_timestamp,
            prev_block_hash: snapshot.l2_block_hash,
            max_virtual_blocks_to_create: 1,
        };

        self.create_batch_executor_inner(storage_factory, l1_batch_env, system_env)
            .await
    }

    pub(super) fn default_batch_params(&self) -> (L1BatchEnv, SystemEnv) {
        // Not really important for the batch executor - it operates over a single batch.
        self.batch_params(L1BatchNumber(1), 100)
    }

    /// Creates test batch params that can be fed into the VM.
    fn batch_params(
        &self,
        l1_batch_number: L1BatchNumber,
        timestamp: u64,
    ) -> (L1BatchEnv, SystemEnv) {
        let mut system_params = default_system_env();
        if let Some(vm_gas_limit) = self.config.vm_gas_limit {
            system_params.bootloader_gas_limit = vm_gas_limit;
        }
        system_params.default_validation_computational_gas_limit =
            self.config.validation_computational_gas_limit;
        let mut batch_params = default_l1_batch_env(l1_batch_number.0, timestamp, self.fee_account);
        batch_params.previous_batch_hash = Some(H256::zero()); // Not important in this context.
        (batch_params, system_params)
    }

    /// Performs the genesis in the storage.
    pub(super) async fn genesis(&self) {
        let mut storage = self.pool.connection_tagged("state_keeper").await.unwrap();
        if storage.blocks_dal().is_genesis_needed().await.unwrap() {
            create_genesis_l1_batch(
                &mut storage,
                ProtocolVersionId::latest(),
                &BASE_SYSTEM_CONTRACTS,
                &get_system_smart_contracts(),
                Default::default(),
            )
            .await
            .unwrap();
        }
    }

    /// Adds funds for specified account list.
    /// Expects genesis to be performed (i.e. `setup_storage` called beforehand).
    pub(super) async fn fund(&self, addresses: &[Address]) {
        let mut storage = self.pool.connection_tagged("state_keeper").await.unwrap();

        let eth_amount = U256::from(10u32).pow(U256::from(32)); //10^32 wei

        for address in addresses {
            let key = storage_key_for_standard_token_balance(
                AccountTreeId::new(L2_BASE_TOKEN_ADDRESS),
                address,
            );
            let value = u256_to_h256(eth_amount);
            let storage_log = StorageLog::new_write_log(key, value);

            storage
                .storage_logs_dal()
                .append_storage_logs(L2BlockNumber(0), &[(H256::zero(), vec![storage_log])])
                .await
                .unwrap();
            if storage
                .storage_logs_dedup_dal()
                .filter_written_slots(&[storage_log.key.hashed_key()])
                .await
                .unwrap()
                .is_empty()
            {
                storage
                    .storage_logs_dedup_dal()
                    .insert_initial_writes(L1BatchNumber(0), &[storage_log.key])
                    .await
                    .unwrap();
            }
        }
    }

    pub(super) async fn wait_for_tasks(&mut self) {
        for task in self.tasks.drain(..) {
            task.await.expect("Failed to join a task");
        }
    }

    pub(super) fn pool(&self) -> ConnectionPool<Core> {
        self.pool.clone()
    }

    pub(super) fn state_keeper_db_path(&self) -> String {
        self.db_dir.path().to_str().unwrap().to_owned()
    }
}

pub trait AccountLoadNextExecutable {
    fn deploy_loadnext_tx(&mut self) -> DeployContractsTx;

    fn l1_execute(&mut self, serial_id: PriorityOpId) -> Transaction;
    /// Returns a valid `execute` transaction.
    /// Automatically increments nonce of the account.
    fn execute(&mut self) -> Transaction;
    fn loadnext_custom_writes_call(
        &mut self,
        address: Address,
        writes: u32,
        gas_limit: u32,
    ) -> Transaction;
    /// Returns a valid `execute` transaction.
    /// Automatically increments nonce of the account.
    fn execute_with_gas_limit(&mut self, gas_limit: u32) -> Transaction;
    /// Returns a transaction to the loadnext contract with custom gas limit and expected burned gas amount.
    /// Increments the account nonce.
    fn loadnext_custom_gas_call(
        &mut self,
        address: Address,
        gas_to_burn: u32,
        gas_limit: u32,
    ) -> Transaction;
}

impl AccountLoadNextExecutable for Account {
    fn deploy_loadnext_tx(&mut self) -> DeployContractsTx {
        let loadnext_contract = get_loadnext_contract();
        let loadnext_constructor_data = &[Token::Uint(U256::from(100))];
        self.get_deploy_tx_with_factory_deps(
            &loadnext_contract.bytecode,
            Some(loadnext_constructor_data),
            loadnext_contract.factory_deps.clone(),
            TxType::L2,
        )
    }
    fn l1_execute(&mut self, serial_id: PriorityOpId) -> Transaction {
        self.get_l1_tx(
            Execute {
                contract_address: Address::random(),
                value: Default::default(),
                calldata: vec![],
                factory_deps: None,
            },
            serial_id.0,
        )
    }

    /// Returns a valid `execute` transaction.
    /// Automatically increments nonce of the account.
    fn execute(&mut self) -> Transaction {
        self.execute_with_gas_limit(1_000_000)
    }

    /// Returns a transaction to the loadnext contract with custom amount of write requests.
    /// Increments the account nonce.
    fn loadnext_custom_writes_call(
        &mut self,
        address: Address,
        writes: u32,
        gas_limit: u32,
    ) -> Transaction {
        // For each iteration of the expensive contract, there are two slots that are updated:
        // the length of the vector and the new slot with the element itself.
        let minimal_fee =
            2 * DEFAULT_GAS_PER_PUBDATA * writes * INITIAL_STORAGE_WRITE_PUBDATA_BYTES as u32;

        let fee = fee(minimal_fee + gas_limit);

        self.get_l2_tx_for_execute(
            Execute {
                contract_address: address,
                calldata: LoadnextContractExecutionParams {
                    reads: 100,
                    writes: writes as usize,
                    events: 100,
                    hashes: 100,
                    recursive_calls: 0,
                    deploys: 100,
                }
                .to_bytes(),
                value: Default::default(),
                factory_deps: None,
            },
            Some(fee),
        )
    }

    /// Returns a valid `execute` transaction.
    /// Automatically increments nonce of the account.
    fn execute_with_gas_limit(&mut self, gas_limit: u32) -> Transaction {
        let fee = fee(gas_limit);
        self.get_l2_tx_for_execute(
            Execute {
                contract_address: Address::random(),
                calldata: vec![],
                value: Default::default(),
                factory_deps: None,
            },
            Some(fee),
        )
    }

    /// Returns a transaction to the loadnext contract with custom gas limit and expected burned gas amount.
    /// Increments the account nonce.
    fn loadnext_custom_gas_call(
        &mut self,
        address: Address,
        gas_to_burn: u32,
        gas_limit: u32,
    ) -> Transaction {
        let fee = fee(gas_limit);
        let calldata = mock_loadnext_gas_burn_calldata(gas_to_burn);

        self.get_l2_tx_for_execute(
            Execute {
                contract_address: address,
                calldata,
                value: Default::default(),
                factory_deps: None,
            },
            Some(fee),
        )
    }
}

fn fee(gas_limit: u32) -> Fee {
    Fee {
        gas_limit: U256::from(gas_limit),
        max_fee_per_gas: SYSTEM_CONTEXT_MINIMAL_BASE_FEE.into(),
        max_priority_fee_per_gas: U256::zero(),
        gas_per_pubdata_limit: U256::from(DEFAULT_GAS_PER_PUBDATA),
    }
}

pub fn mock_loadnext_gas_burn_calldata(gas: u32) -> Vec<u8> {
    let loadnext_contract = get_loadnext_contract();
    let contract_function = loadnext_contract.contract.function("burnGas").unwrap();
    let params = vec![Token::Uint(U256::from(gas))];
    contract_function
        .encode_input(&params)
        .expect("failed to encode parameters")
}

/// Concise representation of a storage snapshot for testing recovery.
#[derive(Debug)]
pub(super) struct StorageSnapshot {
    pub l2_block_number: L2BlockNumber,
    pub l2_block_hash: H256,
    pub l2_block_timestamp: u64,
    pub storage_logs: HashMap<StorageKey, H256>,
    pub factory_deps: HashMap<H256, Vec<u8>>,
}

impl StorageSnapshot {
    /// Generates a new snapshot by executing the specified number of transactions, each in a separate L2 block.
    pub async fn new(
        connection_pool: &ConnectionPool<Core>,
        alice: &mut Account,
        transaction_count: u32,
    ) -> Self {
        let mut tester = Tester::new(connection_pool.clone());
        tester.genesis().await;
        tester.fund(&[alice.address()]).await;

        let mut storage = connection_pool.connection().await.unwrap();
        let all_logs = storage
            .snapshots_creator_dal()
            .get_storage_logs_chunk(
                L2BlockNumber(0),
                L1BatchNumber(0),
                H256::zero()..=H256::repeat_byte(0xff),
            )
            .await
            .unwrap();
        let factory_deps = storage
            .snapshots_creator_dal()
            .get_all_factory_deps(L2BlockNumber(0))
            .await
            .unwrap();
        let mut all_logs: HashMap<_, _> = all_logs
            .into_iter()
            .map(|log| (log.key, log.value))
            .collect();
        drop(storage);

        let executor = tester
            .create_batch_executor(StorageType::AsyncRocksdbCache)
            .await;
        let mut l2_block_env = L2BlockEnv {
            number: 1,
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
            timestamp: 100,
            max_virtual_blocks_to_create: 1,
        };
        let mut storage_writes_deduplicator = StorageWritesDeduplicator::new();

        for _ in 0..transaction_count {
            let tx = alice.execute();
            let tx_hash = tx.hash(); // probably incorrect
            let res = executor.execute_tx(tx).await;
            if let TxExecutionResult::Success { tx_result, .. } = res {
                let storage_logs = &tx_result.logs.storage_logs;
                storage_writes_deduplicator
                    .apply(storage_logs.iter().filter(|log| log.log_query.rw_flag));
            } else {
                panic!("Unexpected tx execution result: {res:?}");
            };

            let mut hasher = L2BlockHasher::new(
                L2BlockNumber(l2_block_env.number),
                l2_block_env.timestamp,
                l2_block_env.prev_block_hash,
            );
            hasher.push_tx_hash(tx_hash);

            l2_block_env.number += 1;
            l2_block_env.timestamp += 1;
            l2_block_env.prev_block_hash = hasher.finalize(ProtocolVersionId::latest());
            executor.start_next_l2_block(l2_block_env).await;
        }

        let finished_batch = executor.finish_batch().await;
        let storage_logs = &finished_batch.block_tip_execution_result.logs.storage_logs;
        storage_writes_deduplicator.apply(storage_logs.iter().filter(|log| log.log_query.rw_flag));
        let modified_entries = storage_writes_deduplicator.into_modified_key_values();
        all_logs.extend(
            modified_entries
                .into_iter()
                .map(|(key, slot)| (key, u256_to_h256(slot.value))),
        );

        // Compute the hash of the last (fictive) L2 block in the batch.
        let l2_block_hash = L2BlockHasher::new(
            L2BlockNumber(l2_block_env.number),
            l2_block_env.timestamp,
            l2_block_env.prev_block_hash,
        )
        .finalize(ProtocolVersionId::latest());

        let mut storage = connection_pool.connection().await.unwrap();
        storage.blocks_dal().delete_genesis().await.unwrap();
        Self {
            l2_block_number: L2BlockNumber(l2_block_env.number),
            l2_block_timestamp: l2_block_env.timestamp,
            l2_block_hash,
            storage_logs: all_logs,
            factory_deps: factory_deps.into_iter().collect(),
        }
    }

    /// Recovers storage from this snapshot.
    pub async fn recover(self, connection_pool: &ConnectionPool<Core>) -> SnapshotRecoveryStatus {
        let snapshot_logs: Vec<_> = self
            .storage_logs
            .into_iter()
            .map(|(key, value)| StorageLog::new_write_log(key, value))
            .collect();
        let mut storage = connection_pool.connection().await.unwrap();
        let mut snapshot = prepare_recovery_snapshot(
            &mut storage,
            L1BatchNumber(1),
            self.l2_block_number,
            &snapshot_logs,
        )
        .await;

        snapshot.l2_block_hash = self.l2_block_hash;
        snapshot.l2_block_timestamp = self.l2_block_timestamp;

        storage
            .factory_deps_dal()
            .insert_factory_deps(snapshot.l2_block_number, &self.factory_deps)
            .await
            .unwrap();
        snapshot
    }
}

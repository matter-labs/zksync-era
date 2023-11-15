//! Testing harness for the batch executor.
//! Contains helper functionality to initialize test context and perform tests without too much boilerplate.

use tempfile::TempDir;

use multivm::interface::{L1BatchEnv, SystemEnv};
use multivm::vm_latest::constants::INITIAL_STORAGE_WRITE_PUBDATA_BYTES;

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_contracts::{get_loadnext_contract, test_contracts::LoadnextContractExecutionParams};
use zksync_dal::ConnectionPool;
use zksync_state::RocksdbStorage;
use zksync_test_account::{Account, DeployContractsTx, TxType};
use zksync_types::{
    ethabi::Token, fee::Fee, system_contracts::get_system_smart_contracts,
    utils::storage_key_for_standard_token_balance, AccountTreeId, Address, Execute, L1BatchNumber,
    L2ChainId, MiniblockNumber, PriorityOpId, ProtocolVersionId, StorageLog, Transaction, H256,
    L2_ETH_TOKEN_ADDRESS, SYSTEM_CONTEXT_MINIMAL_BASE_FEE, U256,
};
use zksync_utils::u256_to_h256;

use crate::genesis::create_genesis_l1_batch;
use crate::state_keeper::{
    batch_executor::BatchExecutorHandle,
    tests::{default_l1_batch_env, default_system_env, BASE_SYSTEM_CONTRACTS},
};

const DEFAULT_GAS_PER_PUBDATA: u32 = 100;
const CHAIN_ID: u32 = 270;

/// Representation of configuration parameters used by the state keeper.
/// Has sensible defaults for most tests, each of which can be overridden.
#[derive(Debug)]
pub(super) struct TestConfig {
    pub(super) save_call_traces: bool,
    pub(super) vm_gas_limit: Option<u32>,
    pub(super) max_allowed_tx_gas_limit: u32,
    pub(super) validation_computational_gas_limit: u32,
    pub(super) upload_witness_inputs_to_gcs: bool,
}

impl TestConfig {
    pub(super) fn new() -> Self {
        let config = StateKeeperConfig::for_tests();

        Self {
            vm_gas_limit: None,
            save_call_traces: false,
            max_allowed_tx_gas_limit: config.max_allowed_l2_tx_gas_limit,
            validation_computational_gas_limit: config.validation_computational_gas_limit,
            upload_witness_inputs_to_gcs: false,
        }
    }
}

/// Tester represents an entity that can initialize the state and create batch executors over this storage.
/// Important: `Tester` must be a *sole* owner of the `ConnectionPool`, since the test pool cannot be shared.
#[derive(Debug)]
pub(super) struct Tester {
    fee_account: Address,
    db_dir: TempDir,
    pool: ConnectionPool,
    config: TestConfig,
}

impl Tester {
    pub(super) fn new(pool: ConnectionPool) -> Self {
        Self::with_config(pool, TestConfig::new())
    }

    pub(super) fn with_config(pool: ConnectionPool, config: TestConfig) -> Self {
        Self {
            fee_account: Address::repeat_byte(0x01),
            db_dir: TempDir::new().unwrap(),
            pool,
            config,
        }
    }

    pub(super) fn set_config(&mut self, config: TestConfig) {
        self.config = config;
    }

    /// Creates a batch executor instance.
    /// This function intentionally uses sensible defaults to not introduce boilerplate.
    pub(super) async fn create_batch_executor(&self) -> BatchExecutorHandle {
        // Not really important for the batch executor - it operates over a single batch.
        let (l1_batch, system_env) = self.batch_params(
            L1BatchNumber(1),
            100,
            self.config.validation_computational_gas_limit,
        );

        let mut secondary_storage = RocksdbStorage::new(self.db_dir.path());
        let mut conn = self
            .pool
            .access_storage_tagged("state_keeper")
            .await
            .unwrap();

        secondary_storage.update_from_postgres(&mut conn).await;
        drop(conn);

        // We don't use the builder because it would require us to clone the `ConnectionPool`, which is forbidden
        // for the test pool (see the doc-comment on `TestPool` for details).
        BatchExecutorHandle::new(
            self.config.save_call_traces,
            self.config.max_allowed_tx_gas_limit.into(),
            secondary_storage,
            l1_batch,
            system_env,
            self.config.upload_witness_inputs_to_gcs,
        )
    }

    /// Creates test batch params that can be fed into the VM.
    fn batch_params(
        &self,
        l1_batch_number: L1BatchNumber,
        timestamp: u64,
        validation_computational_gas_limit: u32,
    ) -> (L1BatchEnv, SystemEnv) {
        let mut system_params = default_system_env();
        if let Some(vm_gas_limit) = self.config.vm_gas_limit {
            system_params.gas_limit = vm_gas_limit;
        }
        system_params.default_validation_computational_gas_limit =
            validation_computational_gas_limit;
        let mut batch_params = default_l1_batch_env(l1_batch_number.0, timestamp, self.fee_account);
        batch_params.previous_batch_hash = Some(H256::zero()); // Not important in this context.
        (batch_params, system_params)
    }

    /// Performs the genesis in the storage.
    pub(super) async fn genesis(&self) {
        let mut storage = self
            .pool
            .access_storage_tagged("state_keeper")
            .await
            .unwrap();
        if storage.blocks_dal().is_genesis_needed().await.unwrap() {
            create_genesis_l1_batch(
                &mut storage,
                self.fee_account,
                L2ChainId::from(CHAIN_ID),
                ProtocolVersionId::latest(),
                &BASE_SYSTEM_CONTRACTS,
                &get_system_smart_contracts(),
                Default::default(),
                Default::default(),
            )
            .await;
        }
    }

    /// Adds funds for specified account list.
    /// Expects genesis to be performed (i.e. `setup_storage` called beforehand).
    pub(super) async fn fund(&self, addresses: &[Address]) {
        let mut storage = self
            .pool
            .access_storage_tagged("state_keeper")
            .await
            .unwrap();

        let eth_amount = U256::from(10u32).pow(U256::from(32)); //10^32 wei

        for address in addresses {
            let key = storage_key_for_standard_token_balance(
                AccountTreeId::new(L2_ETH_TOKEN_ADDRESS),
                address,
            );
            let value = u256_to_h256(eth_amount);
            let storage_log = StorageLog::new_write_log(key, value);

            storage
                .storage_logs_dal()
                .append_storage_logs(MiniblockNumber(0), &[(H256::zero(), vec![storage_log])])
                .await;
            storage
                .storage_dal()
                .apply_storage_logs(&[(H256::zero(), vec![storage_log])])
                .await;
            if storage
                .storage_logs_dedup_dal()
                .filter_written_slots(&[storage_log.key.hashed_key()])
                .await
                .is_empty()
            {
                storage
                    .storage_logs_dedup_dal()
                    .insert_initial_writes(L1BatchNumber(0), &[storage_log.key])
                    .await
            }
        }
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

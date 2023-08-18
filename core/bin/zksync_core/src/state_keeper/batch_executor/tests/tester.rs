//! Testing harness for the batch executor.
//! Contains helper functionality to initialize test context and perform tests without too much boilerplate.

use multivm::VmVersion;
use tempfile::TempDir;

use vm::{
    test_utils::{
        get_create_zksync_address, get_deploy_tx, mock_loadnext_gas_burn_call,
        mock_loadnext_test_call,
    },
    vm_with_bootloader::{BlockContext, BlockContextMode, DerivedBlockContext},
    zk_evm::{
        block_properties::BlockProperties,
        zkevm_opcode_defs::system_params::INITIAL_STORAGE_WRITE_PUBDATA_BYTES,
    },
};
use zksync_config::configs::chain::StateKeeperConfig;

use zksync_contracts::{get_loadnext_contract, TestContract};
use zksync_dal::ConnectionPool;
use zksync_state::RocksdbStorage;
use zksync_types::{
    ethabi::{encode, Token},
    fee::Fee,
    l1::{L1Tx, OpProcessingType, PriorityQueueType},
    l2::L2Tx,
    system_contracts::get_system_smart_contracts,
    utils::storage_key_for_standard_token_balance,
    AccountTreeId, Address, Execute, L1BatchNumber, L1TxCommonData, L2ChainId, MiniblockNumber,
    Nonce, PackedEthSignature, PriorityOpId, ProtocolVersionId, StorageLog, Transaction, H256,
    L2_ETH_TOKEN_ADDRESS, SYSTEM_CONTEXT_MINIMAL_BASE_FEE, U256,
};
use zksync_utils::{test_utils::LoadnextContractExecutionParams, u256_to_h256};

use crate::genesis::create_genesis_l1_batch;
use crate::state_keeper::{
    batch_executor::BatchExecutorHandle,
    io::L1BatchParams,
    tests::{default_block_properties, BASE_SYSTEM_CONTRACTS},
};

const DEFAULT_GAS_PER_PUBDATA: u32 = 100;
const CHAIN_ID: L2ChainId = L2ChainId(270);

/// Representation of configuration parameters used by the state keeper.
/// Has sensible defaults for most tests, each of which can be overridden.
#[derive(Debug)]
pub(super) struct TestConfig {
    pub(super) save_call_traces: bool,
    pub(super) vm_gas_limit: Option<u32>,
    pub(super) max_allowed_tx_gas_limit: u32,
    pub(super) validation_computational_gas_limit: u32,
}

impl TestConfig {
    pub(super) fn new() -> Self {
        // It's OK to use env config here, since we would load the postgres URL from there anyway.
        let config = StateKeeperConfig::from_env();

        Self {
            vm_gas_limit: None,
            save_call_traces: false,
            max_allowed_tx_gas_limit: config.max_allowed_l2_tx_gas_limit,
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
        let (block_context, block_properties) = self.batch_params(L1BatchNumber(1), 100);

        let mut secondary_storage = RocksdbStorage::new(self.db_dir.path());
        let mut conn = self.pool.access_storage_tagged("state_keeper").await;
        secondary_storage.update_from_postgres(&mut conn).await;
        drop(conn);

        // We don't use the builder because it would require us to clone the `ConnectionPool`, which is forbidden
        // for the test pool (see the doc-comment on `TestPool` for details).
        BatchExecutorHandle::new(
            VmVersion::latest(),
            self.config.save_call_traces,
            self.config.max_allowed_tx_gas_limit.into(),
            self.config.validation_computational_gas_limit,
            secondary_storage,
            L1BatchParams {
                context_mode: block_context,
                properties: block_properties,
                base_system_contracts: BASE_SYSTEM_CONTRACTS.clone(),
                protocol_version: ProtocolVersionId::latest(),
            },
            self.config.vm_gas_limit,
        )
    }

    /// Creates test batch params that can be fed into the VM.
    fn batch_params(
        &self,
        l1_batch_number: L1BatchNumber,
        timestamp: u64,
    ) -> (BlockContextMode, BlockProperties) {
        let block_properties = default_block_properties();

        let context = BlockContext {
            block_number: l1_batch_number.0,
            block_timestamp: timestamp,
            l1_gas_price: 1,
            fair_l2_gas_price: 1,
            operator_address: self.fee_account,
        };
        let derived_context = DerivedBlockContext {
            context,
            base_fee: 1,
        };

        let previous_block_hash = U256::zero(); // Not important in this context.
        (
            BlockContextMode::NewBlock(derived_context, previous_block_hash),
            block_properties,
        )
    }

    /// Performs the genesis in the storage.
    pub(super) async fn genesis(&self) {
        let mut storage = self.pool.access_storage_tagged("state_keeper").await;
        if storage.blocks_dal().is_genesis_needed().await {
            create_genesis_l1_batch(
                &mut storage,
                self.fee_account,
                CHAIN_ID,
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
        let mut storage = self.pool.access_storage_tagged("state_keeper").await;

        let eth_amount = U256::from(10u32).pow(U256::from(32)); //10^32 wei

        for address in addresses {
            let key = storage_key_for_standard_token_balance(
                AccountTreeId::new(L2_ETH_TOKEN_ADDRESS),
                address,
            );
            let value = u256_to_h256(eth_amount);
            let storage_logs = vec![StorageLog::new_write_log(key, value)];

            storage
                .storage_logs_dal()
                .append_storage_logs(MiniblockNumber(0), &[(H256::zero(), storage_logs.clone())])
                .await;
            storage
                .storage_dal()
                .apply_storage_logs(&[(H256::zero(), storage_logs)])
                .await;
        }
    }
}

/// Test account that maintains its own nonce and is able to encode common transaction types useful for tests.
#[derive(Debug)]
pub(super) struct Account {
    pub pk: H256,
    pub nonce: Nonce,
}

impl Account {
    pub(super) fn random() -> Self {
        Self {
            pk: H256::random(),
            nonce: Nonce(0),
        }
    }

    /// Returns the address of the account.
    pub(super) fn address(&self) -> Address {
        PackedEthSignature::address_from_private_key(&self.pk).unwrap()
    }

    /// Returns a valid `execute` transaction.
    /// Automatically increments nonce of the account.
    pub(super) fn execute(&mut self) -> Transaction {
        self.execute_with_gas_limit(1_000_000)
    }

    /// Returns a valid `execute` transaction.
    /// Automatically increments nonce of the account.
    pub(super) fn execute_with_gas_limit(&mut self, gas_limit: u32) -> Transaction {
        let fee = fee(gas_limit);
        let mut l2_tx = L2Tx::new_signed(
            Address::random(),
            vec![],
            self.nonce,
            fee,
            Default::default(),
            CHAIN_ID,
            &self.pk,
            None,
            Default::default(),
        )
        .unwrap();
        // Input means all transaction data (NOT calldata, but all tx fields) that came from the API.
        // This input will be used for the derivation of the tx hash, so put some random to it to be sure
        // that the transaction hash is unique.
        l2_tx.set_input(H256::random().0.to_vec(), H256::random());

        // Increment the account nonce.
        self.nonce += 1;

        l2_tx.into()
    }

    /// Returns a valid `execute` transaction initiated from L1.
    /// Does not increment nonce.
    pub(super) fn l1_execute(&mut self, serial_id: PriorityOpId) -> Transaction {
        let execute = Execute {
            contract_address: Address::random(),
            value: Default::default(),
            calldata: vec![],
            factory_deps: None,
        };

        let max_fee_per_gas = U256::from(1u32);
        let gas_limit = U256::from(100_100);
        let priority_op_data = L1TxCommonData {
            sender: self.address(),
            canonical_tx_hash: H256::from_low_u64_be(serial_id.0),
            serial_id,
            deadline_block: 100000,
            layer_2_tip_fee: U256::zero(),
            full_fee: U256::zero(),
            gas_limit,
            max_fee_per_gas,
            op_processing_type: OpProcessingType::Common,
            priority_queue_type: PriorityQueueType::Deque,
            eth_hash: H256::random(),
            eth_block: 1,
            gas_per_pubdata_limit: U256::from(800),
            to_mint: gas_limit * max_fee_per_gas + execute.value,
            refund_recipient: self.address(),
        };

        let tx = L1Tx {
            common_data: priority_op_data,
            execute,
            received_timestamp_ms: 0,
        };
        tx.into()
    }

    /// Returns the transaction to deploy the loadnext contract and address of this contract (after deployment).
    /// Increments the account nonce.
    pub(super) fn deploy_loadnext_tx(&mut self) -> (Transaction, Address) {
        let TestContract {
            bytecode,
            factory_deps,
            ..
        } = get_loadnext_contract();
        let loadnext_deploy_tx = get_deploy_tx(
            self.pk,
            self.nonce,
            &bytecode,
            factory_deps,
            &encode(&[Token::Uint(U256::from(1000))]),
            fee(500_000_000),
        );
        let test_contract_address =
            get_create_zksync_address(loadnext_deploy_tx.initiator_account(), self.nonce);
        self.nonce += 1;

        (loadnext_deploy_tx.into(), test_contract_address)
    }

    /// Returns a transaction to the loadnext contract with custom amount of write requests.
    /// Increments the account nonce.
    pub(super) fn loadnext_custom_writes_call(
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

        let tx = mock_loadnext_test_call(
            self.pk,
            self.nonce,
            address,
            fee,
            LoadnextContractExecutionParams {
                reads: 100,
                writes: writes as usize,
                events: 100,
                hashes: 100,
                recursive_calls: 0,
                deploys: 100,
            },
        );
        self.nonce += 1;
        tx.into()
    }

    /// Returns a transaction to the loadnext contract with custom gas limit and expected burned gas amount.
    /// Increments the account nonce.
    pub(super) fn loadnext_custom_gas_call(
        &mut self,
        address: Address,
        gas_to_burn: u32,
        gas_limit: u32,
    ) -> Transaction {
        let fee = fee(gas_limit);
        let tx = mock_loadnext_gas_burn_call(self.pk, self.nonce, address, fee, gas_to_burn);
        self.nonce += 1;
        tx.into()
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

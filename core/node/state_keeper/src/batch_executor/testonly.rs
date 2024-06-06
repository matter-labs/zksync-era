//! Testing harness for the batch executor.
//! Contains helper functionality to initialize test context and perform tests without too much boilerplate.
use multivm::{
    vm_latest::constants::INITIAL_STORAGE_WRITE_PUBDATA_BYTES,
};
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_contracts::{get_loadnext_contract, test_contracts::LoadnextContractExecutionParams};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_node_genesis::create_genesis_l1_batch;
use zksync_node_test_utils::prepare_recovery_snapshot;
use zksync_state::{ReadStorageFactory, RocksdbStorageOptions};
use zksync_test_account::{Account, DeployContractsTx, TxType};
use zksync_types::{
    block::L2BlockHasher, ethabi::Token, fee::Fee, protocol_version::ProtocolSemanticVersion,
    snapshots::SnapshotRecoveryStatus, storage_writes_deduplicator::StorageWritesDeduplicator,
    system_contracts::get_system_smart_contracts, utils::storage_key_for_standard_token_balance,
    AccountTreeId, Address, Execute, L1BatchNumber, L2BlockNumber, PriorityOpId, ProtocolVersionId,
    StorageKey, StorageLog, Transaction, H256, L2_BASE_TOKEN_ADDRESS,
    SYSTEM_CONTEXT_MINIMAL_BASE_FEE, U256,
};
use zksync_utils::u256_to_h256;

use crate::{
    batch_executor::{BatchExecutorHandle, TxExecutionResult},
    testonly::BASE_SYSTEM_CONTRACTS,
    AsyncRocksdbCache, BatchExecutor, MainBatchExecutor,
};

const DEFAULT_GAS_PER_PUBDATA: u32 = 10000;

/// Adds funds for specified account list.
/// Expects genesis to be performed (i.e. `setup_storage` called beforehand).
pub async fn fund(pool: &ConnectionPool<Core>, addresses: &[Address]) {
    let mut storage = pool.connection().await.unwrap();

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

pub fn mock_loadnext_gas_burn_calldata(gas: u32) -> Vec<u8> {
    let loadnext_contract = get_loadnext_contract();
    let contract_function = loadnext_contract.contract.function("burnGas").unwrap();
    let params = vec![Token::Uint(U256::from(gas))];
    contract_function
        .encode_input(&params)
        .expect("failed to encode parameters")
}

fn fee(gas_limit: u32) -> Fee {
    Fee {
        gas_limit: U256::from(gas_limit),
        max_fee_per_gas: SYSTEM_CONTEXT_MINIMAL_BASE_FEE.into(),
        max_priority_fee_per_gas: U256::zero(),
        gas_per_pubdata_limit: U256::from(DEFAULT_GAS_PER_PUBDATA),
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



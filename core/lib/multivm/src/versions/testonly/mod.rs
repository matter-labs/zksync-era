use ethabi::Contract;
use once_cell::sync::Lazy;
use zksync_contracts::{
    load_contract, read_bytecode, read_yul_bytecode, BaseSystemContracts, SystemContractCode,
};
use zksync_test_account::Account;
use zksync_types::{
    block::L2BlockHasher, fee_model::BatchFeeInput, get_code_key, get_is_account_key,
    helpers::unix_timestamp_ms, utils::storage_key_for_eth_balance, Address, L1BatchNumber,
    L2BlockNumber, L2ChainId, ProtocolVersionId, U256,
};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, u256_to_h256};
use zksync_vm_interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode};

pub(super) use self::tester::TestedVm;
use crate::{
    interface::storage::InMemoryStorage, vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};

pub(super) mod block_tip;
pub(super) mod bootloader;
pub(super) mod bytecode_publishing;
pub(super) mod circuits;
pub(super) mod code_oracle;
pub(super) mod default_aa;
pub(super) mod gas_limit;
pub(super) mod get_used_contracts;
pub(super) mod is_write_initial;
pub(super) mod l1_tx_execution;
pub(super) mod l2_blocks;
pub(super) mod nonce_holder;
pub(super) mod precompiles;
pub(super) mod refunds;
pub(super) mod require_eip712;
pub(super) mod secp256r1;
mod shadow;
pub(super) mod simple_execution;
pub(super) mod storage;
mod tester;

static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

fn get_empty_storage() -> InMemoryStorage {
    InMemoryStorage::with_system_contracts(hash_bytecode)
}

fn read_test_contract() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json")
}

fn get_complex_upgrade_abi() -> Contract {
    load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/complex-upgrade.sol/ComplexUpgrade.json"
    )
}

fn read_complex_upgrade() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/complex-upgrade.sol/ComplexUpgrade.json")
}

fn read_precompiles_contract() -> Vec<u8> {
    read_bytecode(
        "etc/contracts-test-data/artifacts-zk/contracts/precompiles/precompiles.sol/Precompiles.json",
    )
}

fn load_precompiles_contract() -> Contract {
    load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/precompiles/precompiles.sol/Precompiles.json",
    )
}

fn read_proxy_counter_contract() -> (Vec<u8>, Contract) {
    const PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/counter/proxy_counter.sol/ProxyCounter.json";
    (read_bytecode(PATH), load_contract(PATH))
}

fn read_nonce_holder_tester() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/custom-account/nonce-holder-test.sol/NonceHolderTest.json")
}

fn read_expensive_contract() -> (Vec<u8>, Contract) {
    const PATH: &str =
        "etc/contracts-test-data/artifacts-zk/contracts/expensive/expensive.sol/Expensive.json";
    (read_bytecode(PATH), load_contract(PATH))
}

fn read_many_owners_custom_account_contract() -> (Vec<u8>, Contract) {
    let path = "etc/contracts-test-data/artifacts-zk/contracts/custom-account/many-owners-custom-account.sol/ManyOwnersCustomAccount.json";
    (read_bytecode(path), load_contract(path))
}

pub(crate) fn get_bootloader(test: &str) -> SystemContractCode {
    let artifacts_path = "contracts/system-contracts/bootloader/tests/artifacts/";
    let bootloader_code = read_yul_bytecode(artifacts_path, test);

    let bootloader_hash = hash_bytecode(&bootloader_code);
    SystemContractCode {
        code: bytes_to_be_words(bootloader_code),
        hash: bootloader_hash,
    }
}

pub(super) fn default_system_env() -> SystemEnv {
    SystemEnv {
        zk_porter_available: false,
        version: ProtocolVersionId::latest(),
        base_system_smart_contracts: BaseSystemContracts::playground(),
        bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        execution_mode: TxExecutionMode::VerifyExecute,
        default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        chain_id: L2ChainId::from(270),
    }
}

pub(super) fn default_l1_batch(number: L1BatchNumber) -> L1BatchEnv {
    let timestamp = unix_timestamp_ms();
    L1BatchEnv {
        previous_batch_hash: None,
        number,
        timestamp,
        fee_input: BatchFeeInput::l1_pegged(
            50_000_000_000, // 50 gwei
            250_000_000,    // 0.25 gwei
        ),
        fee_account: Address::random(),
        enforced_base_fee: None,
        first_l2_block: L2BlockEnv {
            number: 1,
            timestamp,
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
            max_virtual_blocks_to_create: 100,
        },
    }
}

pub(super) fn make_account_rich(storage: &mut InMemoryStorage, account: &Account) {
    let key = storage_key_for_eth_balance(&account.address);
    storage.set_value(key, u256_to_h256(U256::from(10_u64.pow(19))));
}

#[derive(Debug, Clone)]
pub(super) struct ContractToDeploy {
    bytecode: Vec<u8>,
    address: Address,
    is_account: bool,
}

impl ContractToDeploy {
    pub fn new(bytecode: Vec<u8>, address: Address) -> Self {
        Self {
            bytecode,
            address,
            is_account: false,
        }
    }

    pub fn account(bytecode: Vec<u8>, address: Address) -> Self {
        Self {
            bytecode,
            address,
            is_account: true,
        }
    }

    pub fn insert(&self, storage: &mut InMemoryStorage) {
        let deployer_code_key = get_code_key(&self.address);
        storage.set_value(deployer_code_key, hash_bytecode(&self.bytecode));
        if self.is_account {
            let is_account_key = get_is_account_key(&self.address);
            storage.set_value(is_account_key, u256_to_h256(1_u32.into()));
        }
        storage.store_factory_dep(hash_bytecode(&self.bytecode), self.bytecode.clone());
    }

    /// Inserts the contracts into the test environment, bypassing the deployer system contract.
    pub fn insert_all(contracts: &[Self], storage: &mut InMemoryStorage) {
        for contract in contracts {
            contract.insert(storage);
        }
    }
}

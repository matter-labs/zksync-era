//! Reusable tests and tooling for low-level VM testing.
//!
//! # How it works
//!
//! - [`TestedVm`] defines test-specific VM extensions. It's currently implemented for the latest legacy VM
//!   (`vm_latest`) and the fast VM (`vm_fast`).
//! - Submodules of this module define test functions generic by `TestedVm`. Specific VM versions implement `TestedVm`
//!   and can create tests based on these test functions with minimum amount of boilerplate code.
//! - Tests use [`VmTester`] built using [`VmTesterBuilder`] to create a VM instance. This allows to set up storage for the VM,
//!   custom [`SystemEnv`] / [`L1BatchEnv`], deployed contracts, pre-funded accounts etc.

use std::{collections::HashSet, fs, path::Path, rc::Rc};

use once_cell::sync::Lazy;
use zksync_contracts::{
    read_bootloader_code, read_zbin_bytecode, BaseSystemContracts, SystemContractCode,
};
use zksync_system_constants::{
    CONTRACT_DEPLOYER_ADDRESS, TRUSTED_ADDRESS_SLOTS, TRUSTED_TOKEN_SLOTS,
};
use zksync_types::{
    block::L2BlockHasher,
    bytecode::{pad_evm_bytecode, BytecodeHash},
    fee_model::BatchFeeInput,
    get_code_key, get_evm_code_hash_key, get_is_account_key, get_known_code_key, h256_to_address,
    h256_to_u256, u256_to_h256,
    utils::storage_key_for_eth_balance,
    web3, Address, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId, Transaction, H256,
    U256,
};

pub(super) use self::tester::{
    validation_params, TestedVm, TestedVmForValidation, TestedVmWithCallTracer,
    TestedVmWithStorageLimit, VmTester, VmTesterBuilder,
};
use crate::{
    interface::{
        pubdata::PubdataBuilder,
        storage::{InMemoryStorage, StorageSnapshot, StorageView},
        tracer::ValidationParams,
        utils::VmDump,
        InspectExecutionMode, L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode, VmEvent,
        VmExecutionResultAndLogs, VmFactory,
    },
    pubdata_builders::FullPubdataBuilder,
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};

pub(super) mod account_validation_rules;
pub(super) mod block_tip;
pub(super) mod bootloader;
pub(super) mod bytecode_publishing;
pub(super) mod call_tracer;
pub(super) mod circuits;
pub(super) mod code_oracle;
pub(super) mod default_aa;
pub(super) mod evm;
pub(super) mod gas_limit;
pub(super) mod get_used_contracts;
pub(super) mod is_write_initial;
pub(super) mod l1_messenger;
pub(super) mod l1_tx_execution;
pub(super) mod l2_blocks;
pub(super) mod mock_evm;
pub(super) mod nonce_holder;
pub(super) mod precompiles;
pub(super) mod refunds;
pub(super) mod require_eip712;
pub(super) mod rollbacks;
pub(super) mod secp256r1;
pub(super) mod simple_execution;
pub(super) mod storage;
mod tester;
pub(super) mod tracing_execution_error;
pub(super) mod transfer;
pub(super) mod upgrade;

static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

fn get_empty_storage() -> InMemoryStorage {
    InMemoryStorage::with_system_contracts()
}

pub(crate) fn read_max_depth_contract() -> Vec<u8> {
    read_zbin_bytecode(
        "core/tests/ts-integration/contracts/zkasm/artifacts/deep_stak.zkasm/deep_stak.zkasm.zbin",
    )
}

pub(crate) fn load_vm_dump(name: &str) -> VmDump {
    // We rely on the fact that unit tests are executed from the crate directory.
    let path = Path::new("tests/vm_dumps").join(format!("{name}.json"));
    let raw = fs::read_to_string(path).unwrap_or_else(|err| {
        panic!("Failed reading VM dump `{name}`: {err}");
    });
    serde_json::from_str(&raw).unwrap_or_else(|err| {
        panic!("Failed deserializing VM dump `{name}`: {err}");
    })
}

pub(crate) fn inspect_oneshot_dump<VM>(
    dump: VmDump,
    tracer: &mut VM::TracerDispatcher,
) -> VmExecutionResultAndLogs
where
    VM: VmFactory<StorageView<StorageSnapshot>>,
{
    let storage = StorageView::new(dump.storage).to_rc_ptr();

    assert_eq!(dump.l2_blocks.len(), 1);
    let transactions = dump.l2_blocks.into_iter().next().unwrap().txs;
    assert_eq!(transactions.len(), 1);
    let transaction = transactions.into_iter().next().unwrap();

    let mut vm = VM::new(dump.l1_batch_env, dump.system_env, storage);
    vm.push_transaction(transaction);
    vm.inspect(tracer, InspectExecutionMode::OneTx)
}

pub(crate) fn execute_oneshot_dump<VM>(dump: VmDump) -> VmExecutionResultAndLogs
where
    VM: VmFactory<StorageView<StorageSnapshot>>,
{
    inspect_oneshot_dump::<VM>(dump, &mut <VM::TracerDispatcher>::default())
}

pub(crate) fn mock_validation_params(
    tx: &Transaction,
    accessed_tokens: &[Address],
) -> ValidationParams {
    let trusted_slots = accessed_tokens
        .iter()
        .flat_map(|&addr| TRUSTED_TOKEN_SLOTS.iter().map(move |&slot| (addr, slot)))
        .collect();
    let trusted_address_slots = accessed_tokens
        .iter()
        .flat_map(|&addr| TRUSTED_ADDRESS_SLOTS.iter().map(move |&slot| (addr, slot)))
        .collect();
    ValidationParams {
        user_address: tx.initiator_account(),
        paymaster_address: tx.payer(),
        trusted_slots,
        trusted_addresses: Default::default(),
        trusted_address_slots,
        computational_gas_limit: u32::MAX,
        timestamp_asserter_params: None,
    }
}

pub(crate) fn get_bootloader(test: &str) -> SystemContractCode {
    let bootloader_code = read_bootloader_code(test);
    let bootloader_hash = BytecodeHash::for_bytecode(&bootloader_code).value();
    SystemContractCode {
        code: bootloader_code,
        hash: bootloader_hash,
    }
}

pub(crate) fn filter_out_base_system_contracts(all_bytecode_hashes: &mut HashSet<U256>) {
    all_bytecode_hashes.remove(&h256_to_u256(BASE_SYSTEM_CONTRACTS.default_aa.hash));
    if let Some(evm_emulator) = &BASE_SYSTEM_CONTRACTS.evm_emulator {
        all_bytecode_hashes.remove(&h256_to_u256(evm_emulator.hash));
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
    // Add a bias to the timestamp to make it more realistic / "random".
    let timestamp = 1_700_000_000 + u64::from(number.0);
    L1BatchEnv {
        previous_batch_hash: None,
        number,
        timestamp,
        fee_input: BatchFeeInput::l1_pegged(
            50_000_000_000, // 50 gwei
            250_000_000,    // 0.25 gwei
        ),
        fee_account: Address::repeat_byte(1),
        enforced_base_fee: None,
        first_l2_block: L2BlockEnv {
            number: 1,
            timestamp,
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
            max_virtual_blocks_to_create: 100,
            interop_roots: vec![],
        },
    }
}

pub(super) fn default_pubdata_builder() -> Rc<dyn PubdataBuilder> {
    Rc::new(FullPubdataBuilder::new(Some(Address::zero()), None))
}

pub(super) fn make_address_rich(storage: &mut InMemoryStorage, address: Address) {
    let key = storage_key_for_eth_balance(&address);
    storage.set_value(key, u256_to_h256(U256::from(10_u64.pow(19))));
}

#[derive(Debug, Clone)]
pub(super) struct ContractToDeploy {
    bytecode: Vec<u8>,
    address: Address,
    is_account: bool,
    is_funded: bool,
}

impl ContractToDeploy {
    pub fn new(bytecode: Vec<u8>, address: Address) -> Self {
        Self {
            bytecode,
            address,
            is_account: false,
            is_funded: false,
        }
    }

    pub fn account(bytecode: Vec<u8>, address: Address) -> Self {
        Self {
            bytecode,
            address,
            is_account: true,
            is_funded: false,
        }
    }

    #[must_use]
    pub fn funded(mut self) -> Self {
        self.is_funded = true;
        self
    }

    pub fn insert(&self, storage: &mut InMemoryStorage) {
        let deployer_code_key = get_code_key(&self.address);
        let bytecode_hash = BytecodeHash::for_bytecode(&self.bytecode).value();
        storage.set_value(deployer_code_key, bytecode_hash);
        if self.is_account {
            let is_account_key = get_is_account_key(&self.address);
            storage.set_value(is_account_key, u256_to_h256(1_u32.into()));
        }
        storage.store_factory_dep(bytecode_hash, self.bytecode.clone());

        if self.is_funded {
            make_address_rich(storage, self.address);
        }
    }

    pub fn insert_evm(&self, storage: &mut InMemoryStorage) {
        let evm_bytecode_keccak_hash = H256(web3::keccak256(&self.bytecode));
        let padded_evm_bytecode = pad_evm_bytecode(&self.bytecode);
        let evm_bytecode_hash =
            BytecodeHash::for_evm_bytecode(self.bytecode.len(), &padded_evm_bytecode).value();

        // Mark the EVM contract as deployed.
        storage.set_value(
            get_known_code_key(&evm_bytecode_hash),
            H256::from_low_u64_be(1),
        );
        storage.set_value(get_code_key(&self.address), evm_bytecode_hash);
        storage.set_value(
            get_evm_code_hash_key(evm_bytecode_hash),
            evm_bytecode_keccak_hash,
        );
        storage.store_factory_dep(evm_bytecode_hash, padded_evm_bytecode);

        if self.is_funded {
            make_address_rich(storage, self.address);
        }
    }
}

fn extract_deploy_events(events: &[VmEvent]) -> Vec<(Address, Address)> {
    events
        .iter()
        .filter_map(|event| {
            if event.address == CONTRACT_DEPLOYER_ADDRESS
                && event.indexed_topics[0] == VmEvent::DEPLOY_EVENT_SIGNATURE
            {
                let deployer = h256_to_address(&event.indexed_topics[1]);
                let deployed_address = h256_to_address(&event.indexed_topics[3]);
                Some((deployer, deployed_address))
            } else {
                None
            }
        })
        .collect()
}

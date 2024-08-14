use std::{cell::RefCell, rc::Rc};

use once_cell::sync::Lazy;
pub use zksync_contracts::test_contracts::LoadnextContractExecutionParams as LoadTestParams;
use zksync_contracts::{deployer_contract, BaseSystemContracts, TestContract};
use zksync_multivm::{
    interface::{
        storage::{InMemoryStorage, StorageView},
        ExecutionResult, L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
    },
    utils::get_max_gas_per_pubdata_byte,
    vm_fast, vm_latest,
    vm_latest::{constants::BATCH_COMPUTATIONAL_GAS_LIMIT, HistoryEnabled},
};
use zksync_types::{
    block::L2BlockHasher,
    ethabi::{encode, Token},
    fee::Fee,
    fee_model::BatchFeeInput,
    helpers::unix_timestamp_ms,
    l2::L2Tx,
    utils::{deployed_address_create, storage_key_for_eth_balance},
    Address, K256PrivateKey, L1BatchNumber, L2BlockNumber, L2ChainId, Nonce, ProtocolVersionId,
    Transaction, CONTRACT_DEPLOYER_ADDRESS, H256, U256,
};
use zksync_utils::bytecode::hash_bytecode;

mod instruction_counter;

/// Bytecodes have consist of an odd number of 32 byte words
/// This function "fixes" bytecodes of wrong length by cutting off their end.
pub fn cut_to_allowed_bytecode_size(bytes: &[u8]) -> Option<&[u8]> {
    let mut words = bytes.len() / 32;
    if words == 0 {
        return None;
    }

    if words & 1 == 0 {
        words -= 1;
    }
    Some(&bytes[..32 * words])
}

const LOAD_TEST_MAX_READS: usize = 100;

static LOAD_TEST_CONTRACT_ADDRESS: Lazy<Address> =
    Lazy::new(|| deployed_address_create(PRIVATE_KEY.address(), 0.into()));

static STORAGE: Lazy<InMemoryStorage> = Lazy::new(|| {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    // Give `PRIVATE_KEY` some money
    let balance = U256::from(10u32).pow(U256::from(32)); //10^32 wei
    let key = storage_key_for_eth_balance(&PRIVATE_KEY.address());
    storage.set_value(key, zksync_utils::u256_to_h256(balance));
    storage
});

static SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> = Lazy::new(BaseSystemContracts::load_from_disk);

static LOAD_TEST_CONTRACT: Lazy<TestContract> = Lazy::new(zksync_contracts::get_loadnext_contract);

static CREATE_FUNCTION_SIGNATURE: Lazy<[u8; 4]> = Lazy::new(|| {
    deployer_contract()
        .function("create")
        .unwrap()
        .short_signature()
});

static PRIVATE_KEY: Lazy<K256PrivateKey> =
    Lazy::new(|| K256PrivateKey::from_bytes(H256([42; 32])).expect("invalid key bytes"));

/// VM label used to name `criterion` benchmarks.
#[derive(Debug, Clone, Copy)]
pub enum VmLabel {
    Fast,
    Legacy,
}

impl VmLabel {
    /// Non-empty name for `criterion` benchmark naming.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Legacy => "legacy",
        }
    }

    /// Optional prefix for `criterion` benchmark naming (including a starting `/`).
    pub const fn as_suffix(self) -> &'static str {
        match self {
            Self::Fast => "",
            Self::Legacy => "/legacy",
        }
    }
}

/// Factory for VMs used in benchmarking.
pub trait BenchmarkingVmFactory {
    /// VM label used to name `criterion` benchmarks.
    const LABEL: VmLabel;

    /// Type of the VM instance created by this factory.
    type Instance: VmInterfaceHistoryEnabled;

    /// Creates a VM instance.
    fn create(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: &'static InMemoryStorage,
    ) -> Self::Instance;
}

/// Factory for the new / fast VM.
#[derive(Debug)]
pub struct Fast(());

impl BenchmarkingVmFactory for Fast {
    const LABEL: VmLabel = VmLabel::Fast;

    type Instance = vm_fast::Vm<&'static InMemoryStorage>;

    fn create(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: &'static InMemoryStorage,
    ) -> Self::Instance {
        vm_fast::Vm::new(batch_env, system_env, storage)
    }
}

/// Factory for the legacy VM (latest version).
#[derive(Debug)]
pub struct Legacy;

impl BenchmarkingVmFactory for Legacy {
    const LABEL: VmLabel = VmLabel::Legacy;

    type Instance = vm_latest::Vm<StorageView<&'static InMemoryStorage>, HistoryEnabled>;

    fn create(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: &'static InMemoryStorage,
    ) -> Self::Instance {
        let storage = StorageView::new(storage).to_rc_ptr();
        vm_latest::Vm::new(batch_env, system_env, storage)
    }
}

#[derive(Debug)]
pub struct BenchmarkingVm<VM: BenchmarkingVmFactory>(VM::Instance);

impl<VM: BenchmarkingVmFactory> Default for BenchmarkingVm<VM> {
    fn default() -> Self {
        let timestamp = unix_timestamp_ms();
        Self(VM::create(
            L1BatchEnv {
                previous_batch_hash: None,
                number: L1BatchNumber(1),
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
            },
            SystemEnv {
                zk_porter_available: false,
                version: ProtocolVersionId::latest(),
                base_system_smart_contracts: SYSTEM_CONTRACTS.clone(),
                bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
                execution_mode: TxExecutionMode::VerifyExecute,
                default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
                chain_id: L2ChainId::from(270),
            },
            &STORAGE,
        ))
    }
}

impl<VM: BenchmarkingVmFactory> BenchmarkingVm<VM> {
    pub fn run_transaction(&mut self, tx: &Transaction) -> VmExecutionResultAndLogs {
        self.0.push_transaction(tx.clone());
        self.0.execute(VmExecutionMode::OneTx)
    }

    pub fn run_transaction_full(&mut self, tx: &Transaction) -> VmExecutionResultAndLogs {
        self.0.make_snapshot();
        let (compression_result, tx_result) = self.0.inspect_transaction_with_bytecode_compression(
            Default::default(),
            tx.clone(),
            true,
        );
        compression_result.expect("compressing bytecodes failed");

        if matches!(tx_result.result, ExecutionResult::Halt { .. }) {
            self.0.rollback_to_the_latest_snapshot();
        } else {
            self.0.pop_snapshot_no_rollback();
        }
        tx_result
    }

    pub fn instruction_count(&mut self, tx: &Transaction) -> usize {
        self.0.push_transaction(tx.clone());
        let count = Rc::new(RefCell::new(0));
        self.0.inspect(Default::default(), VmExecutionMode::OneTx); // FIXME: re-enable instruction counting once new tracers are merged
        count.take()
    }
}

impl BenchmarkingVm<Fast> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BenchmarkingVm<Legacy> {
    pub fn legacy() -> Self {
        Self::default()
    }
}

pub fn get_deploy_tx(code: &[u8]) -> Transaction {
    get_deploy_tx_with_gas_limit(code, 30_000_000, 0)
}

pub fn get_deploy_tx_with_gas_limit(code: &[u8], gas_limit: u32, nonce: u32) -> Transaction {
    let mut salt = vec![0_u8; 32];
    salt[28..32].copy_from_slice(&nonce.to_be_bytes());
    let params = [
        Token::FixedBytes(salt),
        Token::FixedBytes(hash_bytecode(code).0.to_vec()),
        Token::Bytes([].to_vec()),
    ];
    let calldata = CREATE_FUNCTION_SIGNATURE
        .iter()
        .cloned()
        .chain(encode(&params))
        .collect();

    let mut signed = L2Tx::new_signed(
        CONTRACT_DEPLOYER_ADDRESS,
        calldata,
        Nonce(nonce),
        tx_fee(gas_limit),
        U256::zero(),
        L2ChainId::from(270),
        &PRIVATE_KEY,
        vec![code.to_vec()], // maybe not needed?
        Default::default(),
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());
    signed.into()
}

fn tx_fee(gas_limit: u32) -> Fee {
    Fee {
        gas_limit: U256::from(gas_limit),
        max_fee_per_gas: U256::from(250_000_000),
        max_priority_fee_per_gas: U256::from(0),
        gas_per_pubdata_limit: U256::from(get_max_gas_per_pubdata_byte(
            ProtocolVersionId::latest().into(),
        )),
    }
}

pub fn get_transfer_tx(nonce: u32) -> Transaction {
    let mut signed = L2Tx::new_signed(
        PRIVATE_KEY.address(),
        vec![], // calldata
        Nonce(nonce),
        tx_fee(1_000_000),
        1_000_000_000.into(), // value
        L2ChainId::from(270),
        &PRIVATE_KEY,
        vec![],             // factory deps
        Default::default(), // paymaster params
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());
    signed.into()
}

pub fn get_load_test_deploy_tx() -> Transaction {
    let calldata = [Token::Uint(LOAD_TEST_MAX_READS.into())];
    let params = [
        Token::FixedBytes(vec![0_u8; 32]),
        Token::FixedBytes(hash_bytecode(&LOAD_TEST_CONTRACT.bytecode).0.to_vec()),
        Token::Bytes(encode(&calldata)),
    ];
    let create_calldata = CREATE_FUNCTION_SIGNATURE
        .iter()
        .cloned()
        .chain(encode(&params))
        .collect();

    let mut factory_deps = LOAD_TEST_CONTRACT.factory_deps.clone();
    factory_deps.push(LOAD_TEST_CONTRACT.bytecode.clone());

    let mut signed = L2Tx::new_signed(
        CONTRACT_DEPLOYER_ADDRESS,
        create_calldata,
        Nonce(0),
        tx_fee(100_000_000),
        U256::zero(),
        L2ChainId::from(270),
        &PRIVATE_KEY,
        factory_deps,
        Default::default(),
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());
    signed.into()
}

pub fn get_load_test_tx(nonce: u32, gas_limit: u32, params: LoadTestParams) -> Transaction {
    assert!(
        params.reads <= LOAD_TEST_MAX_READS,
        "Too many reads: {params:?}, should be <={LOAD_TEST_MAX_READS}"
    );

    let execute_function = LOAD_TEST_CONTRACT
        .contract
        .function("execute")
        .expect("no `execute` function in load test contract");
    let calldata = execute_function
        .encode_input(&vec![
            Token::Uint(U256::from(params.reads)),
            Token::Uint(U256::from(params.new_writes)),
            Token::Uint(U256::from(params.over_writes)),
            Token::Uint(U256::from(params.hashes)),
            Token::Uint(U256::from(params.events)),
            Token::Uint(U256::from(params.recursive_calls)),
            Token::Uint(U256::from(params.deploys)),
        ])
        .expect("cannot encode `execute` inputs");

    let mut signed = L2Tx::new_signed(
        *LOAD_TEST_CONTRACT_ADDRESS,
        calldata,
        Nonce(nonce),
        tx_fee(gas_limit),
        U256::zero(),
        L2ChainId::from(270),
        &PRIVATE_KEY,
        LOAD_TEST_CONTRACT.factory_deps.clone(),
        Default::default(),
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());
    signed.into()
}

pub fn get_realistic_load_test_tx(nonce: u32) -> Transaction {
    get_load_test_tx(
        nonce,
        10_000_000,
        LoadTestParams {
            reads: 30,
            new_writes: 2,
            over_writes: 2,
            events: 5,
            hashes: 10,
            recursive_calls: 0,
            deploys: 0,
        },
    )
}

pub fn get_heavy_load_test_tx(nonce: u32) -> Transaction {
    get_load_test_tx(
        nonce,
        10_000_000,
        LoadTestParams {
            reads: 100,
            new_writes: 5,
            over_writes: 5,
            events: 20,
            hashes: 100,
            recursive_calls: 20,
            deploys: 5,
        },
    )
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use zksync_contracts::read_bytecode;
    use zksync_multivm::interface::ExecutionResult;

    use crate::*;

    #[test]
    fn can_deploy_contract() {
        let test_contract = read_bytecode(
            "etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json",
        );
        let mut vm = BenchmarkingVm::new();
        let res = vm.run_transaction(&get_deploy_tx(&test_contract));

        assert_matches!(res.result, ExecutionResult::Success { .. });
    }

    #[test]
    fn can_transfer() {
        let mut vm = BenchmarkingVm::new();
        let res = vm.run_transaction(&get_transfer_tx(0));
        assert_matches!(res.result, ExecutionResult::Success { .. });
    }

    #[test]
    fn can_load_test() {
        let mut vm = BenchmarkingVm::new();
        let res = vm.run_transaction(&get_load_test_deploy_tx());
        assert_matches!(res.result, ExecutionResult::Success { .. });

        let params = LoadTestParams::default();
        let res = vm.run_transaction(&get_load_test_tx(1, 10_000_000, params));
        assert_matches!(res.result, ExecutionResult::Success { .. });
    }

    #[test]
    fn can_load_test_with_realistic_txs() {
        let mut vm = BenchmarkingVm::new();
        let res = vm.run_transaction(&get_load_test_deploy_tx());
        assert_matches!(res.result, ExecutionResult::Success { .. });

        let res = vm.run_transaction(&get_realistic_load_test_tx(1));
        assert_matches!(res.result, ExecutionResult::Success { .. });
    }

    #[test]
    fn can_load_test_with_heavy_txs() {
        let mut vm = BenchmarkingVm::new();
        let res = vm.run_transaction(&get_load_test_deploy_tx());
        assert_matches!(res.result, ExecutionResult::Success { .. });

        let res = vm.run_transaction(&get_heavy_load_test_tx(1));
        assert_matches!(res.result, ExecutionResult::Success { .. });
    }
}

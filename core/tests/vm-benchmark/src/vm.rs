use std::{cell::RefCell, rc::Rc};

use once_cell::sync::Lazy;
use zksync_contracts::BaseSystemContracts;
use zksync_multivm::{
    interface::{
        storage::{InMemoryStorage, StorageView},
        ExecutionResult, L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
    },
    vm_fast, vm_latest,
    vm_latest::{constants::BATCH_COMPUTATIONAL_GAS_LIMIT, HistoryEnabled},
    zk_evm_latest::ethereum_types::{Address, U256},
};
use zksync_types::{
    block::L2BlockHasher, fee_model::BatchFeeInput, helpers::unix_timestamp_ms,
    utils::storage_key_for_eth_balance, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId,
    Transaction,
};
use zksync_utils::bytecode::hash_bytecode;

use crate::transaction::PRIVATE_KEY;

static SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> = Lazy::new(BaseSystemContracts::load_from_disk);

static STORAGE: Lazy<InMemoryStorage> = Lazy::new(|| {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    // Give `PRIVATE_KEY` some money
    let balance = U256::from(10u32).pow(U256::from(32)); //10^32 wei
    let key = storage_key_for_eth_balance(&PRIVATE_KEY.address());
    storage.set_value(key, zksync_utils::u256_to_h256(balance));
    storage
});

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
                pubdata_params: Default::default(),
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

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use zksync_contracts::read_bytecode;
    use zksync_multivm::interface::ExecutionResult;

    use super::*;
    use crate::{
        get_deploy_tx, get_heavy_load_test_tx, get_load_test_deploy_tx, get_load_test_tx,
        get_realistic_load_test_tx, get_transfer_tx, LoadTestParams,
    };

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

use std::{cell::RefCell, rc::Rc};

use once_cell::sync::Lazy;
use zksync_contracts::BaseSystemContracts;
use zksync_multivm::{
    interface::{
        storage::{ImmutableStorageView, InMemoryStorage, StoragePtr, StorageView},
        ExecutionResult, InspectExecutionMode, L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
    },
    vm_fast::{self, FastValidationTracer, StorageInvocationsTracer},
    vm_latest::{self, constants::BATCH_COMPUTATIONAL_GAS_LIMIT, HistoryEnabled, ToTracerPointer},
    zk_evm_latest::ethereum_types::{Address, U256},
};
use zksync_types::{
    block::L2BlockHasher, fee_model::BatchFeeInput, helpers::unix_timestamp_ms, u256_to_h256,
    utils::storage_key_for_eth_balance, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId,
    Transaction,
};

use crate::{instruction_counter::InstructionCounter, transaction::PRIVATE_KEY};

static SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> = Lazy::new(BaseSystemContracts::load_from_disk);

static STORAGE: Lazy<InMemoryStorage> = Lazy::new(|| {
    let mut storage = InMemoryStorage::with_system_contracts();
    // Give `PRIVATE_KEY` some money
    let balance = U256::from(10u32).pow(U256::from(32)); //10^32 wei
    let key = storage_key_for_eth_balance(&PRIVATE_KEY.address());
    storage.set_value(key, u256_to_h256(balance));
    storage
});

/// VM label used to name `criterion` benchmarks.
#[derive(Debug, Clone, Copy)]
pub enum VmLabel {
    Fast,
    FastNoSignatures,
    FastWithStorageLimit,
    Legacy,
}

impl VmLabel {
    /// Non-empty name for `criterion` benchmark naming.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::FastNoSignatures => "fast_no_sigs",
            Self::FastWithStorageLimit => "fast/storage_limit",
            Self::Legacy => "legacy",
        }
    }

    /// Optional prefix for `criterion` benchmark naming (including a starting `/`).
    pub const fn as_suffix(self) -> &'static str {
        match self {
            Self::Fast => "",
            Self::FastNoSignatures => "/no_sigs",
            Self::FastWithStorageLimit => "/storage_limit",
            Self::Legacy => "/legacy",
        }
    }
}

/// Factory for VMs used in benchmarking.
pub trait BenchmarkingVmFactory: Sized {
    /// VM label used to name `criterion` benchmarks.
    const LABEL: VmLabel;

    /// Type of the VM instance created by this factory.
    type Instance: VmInterfaceHistoryEnabled;

    /// Creates a VM instance.
    fn create(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: &'static InMemoryStorage,
    ) -> (Self, Self::Instance);

    fn create_tracer(&self) -> <Self::Instance as VmInterface>::TracerDispatcher {
        <Self::Instance as VmInterface>::TracerDispatcher::default()
    }
}

pub trait CountInstructions {
    /// Counts instructions executed by the VM while processing the transaction.
    fn count_instructions(tx: &Transaction) -> usize;
}

/// Factory for the new / fast VM.
#[derive(Debug)]
pub struct Fast;

impl BenchmarkingVmFactory for Fast {
    const LABEL: VmLabel = VmLabel::Fast;

    type Instance = vm_fast::Vm<&'static InMemoryStorage>;

    fn create(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: &'static InMemoryStorage,
    ) -> (Self, Self::Instance) {
        (Self, vm_fast::Vm::custom(batch_env, system_env, storage))
    }
}

impl CountInstructions for Fast {
    fn count_instructions(tx: &Transaction) -> usize {
        use vm_fast::interface as vm2;

        #[derive(Default)]
        struct InstructionCount(usize);

        impl vm2::Tracer for InstructionCount {
            fn before_instruction<OP: vm2::OpcodeType, S: vm2::GlobalStateInterface>(
                &mut self,
                _: &mut S,
            ) {
                self.0 += 1;
            }
        }

        let (system_env, l1_batch_env) = test_env();
        let mut vm = vm_fast::Vm::custom(l1_batch_env, system_env, &*STORAGE);
        vm.push_transaction(tx.clone());
        let mut tracer = (InstructionCount(0), FastValidationTracer::default());
        vm.inspect(&mut tracer, InspectExecutionMode::OneTx);
        tracer.0 .0
    }
}

#[derive(Debug)]
pub struct FastNoSignatures;

impl BenchmarkingVmFactory for FastNoSignatures {
    const LABEL: VmLabel = VmLabel::FastNoSignatures;

    type Instance = vm_fast::Vm<&'static InMemoryStorage>;

    fn create(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: &'static InMemoryStorage,
    ) -> (Self, Self::Instance) {
        let mut vm = vm_fast::Vm::custom(batch_env, system_env, storage);
        vm.skip_signature_verification();
        (Self, vm)
    }
}

#[derive(Debug)]
pub struct FastWithStorageLimit {
    storage: StoragePtr<StorageView<&'static InMemoryStorage>>,
}

impl BenchmarkingVmFactory for FastWithStorageLimit {
    const LABEL: VmLabel = VmLabel::FastWithStorageLimit;

    type Instance = vm_fast::Vm<
        ImmutableStorageView<&'static InMemoryStorage>,
        StorageInvocationsTracer<StorageView<&'static InMemoryStorage>>,
    >;

    fn create(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: &'static InMemoryStorage,
    ) -> (Self, Self::Instance) {
        let storage = StorageView::new(storage).to_rc_ptr();
        let this = Self {
            storage: storage.clone(),
        };
        (this, vm_fast::Vm::new(batch_env, system_env, storage))
    }

    fn create_tracer(&self) -> <Self::Instance as VmInterface>::TracerDispatcher {
        // Set some large limit so that tx execution isn't affected by it.
        let limit = u32::MAX as usize / 2;
        (
            StorageInvocationsTracer::new(self.storage.clone(), limit),
            FastValidationTracer::default(),
        )
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
    ) -> (Self, Self::Instance) {
        let storage = StorageView::new(storage).to_rc_ptr();
        (Self, vm_latest::Vm::new(batch_env, system_env, storage))
    }
}

impl CountInstructions for Legacy {
    fn count_instructions(tx: &Transaction) -> usize {
        let mut vm = BenchmarkingVm::<Self>::default();
        vm.vm.push_transaction(tx.clone());
        let count = Rc::new(RefCell::new(0));
        vm.vm.inspect(
            &mut InstructionCounter::new(count.clone())
                .into_tracer_pointer()
                .into(),
            InspectExecutionMode::OneTx,
        );
        count.take()
    }
}

fn test_env() -> (SystemEnv, L1BatchEnv) {
    let timestamp = unix_timestamp_ms();
    let system_env = SystemEnv {
        zk_porter_available: false,
        version: ProtocolVersionId::latest(),
        base_system_smart_contracts: SYSTEM_CONTRACTS.clone(),
        bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        execution_mode: TxExecutionMode::VerifyExecute,
        default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        chain_id: L2ChainId::from(270),
    };
    let l1_batch_env = L1BatchEnv {
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
            interop_roots: vec![],
        },
    };
    (system_env, l1_batch_env)
}

#[derive(Debug)]
pub struct BenchmarkingVm<VM: BenchmarkingVmFactory> {
    factory: VM,
    vm: VM::Instance,
}

impl<VM: BenchmarkingVmFactory> Default for BenchmarkingVm<VM> {
    fn default() -> Self {
        let (system_env, l1_batch_env) = test_env();
        let (factory, vm) = VM::create(l1_batch_env, system_env, &STORAGE);
        Self { factory, vm }
    }
}

impl<VM: BenchmarkingVmFactory> BenchmarkingVm<VM> {
    pub fn run_transaction(&mut self, tx: &Transaction) -> VmExecutionResultAndLogs {
        self.vm.push_transaction(tx.clone());
        self.vm.inspect(
            &mut self.factory.create_tracer(),
            InspectExecutionMode::OneTx,
        )
    }

    pub fn run_transaction_full(&mut self, tx: &Transaction) -> VmExecutionResultAndLogs {
        self.vm.make_snapshot();
        let (compression_result, tx_result) =
            self.vm.inspect_transaction_with_bytecode_compression(
                &mut self.factory.create_tracer(),
                tx.clone(),
                true,
            );
        compression_result.expect("compressing bytecodes failed");

        if matches!(tx_result.result, ExecutionResult::Halt { .. }) {
            self.vm.rollback_to_the_latest_snapshot();
        } else {
            self.vm.pop_snapshot_no_rollback();
        }
        tx_result
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
    use zksync_multivm::interface::ExecutionResult;
    use zksync_test_contracts::TestContract;

    use super::*;
    use crate::{
        get_deploy_tx, get_heavy_load_test_tx, get_load_test_deploy_tx, get_load_test_tx,
        get_realistic_load_test_tx, get_transfer_tx,
        transaction::{get_erc20_deploy_tx, get_erc20_transfer_tx},
        LoadTestParams, BYTECODES,
    };

    #[test]
    fn can_deploy_contract() {
        let test_contract = &TestContract::counter().bytecode;
        let mut vm = BenchmarkingVm::new();
        let res = vm.run_transaction(&get_deploy_tx(test_contract));

        assert_matches!(res.result, ExecutionResult::Success { .. });
    }

    #[test]
    fn can_transfer() {
        let mut vm = BenchmarkingVm::new();
        let res = vm.run_transaction(&get_transfer_tx(0));
        assert_matches!(res.result, ExecutionResult::Success { .. });
    }

    #[test]
    fn can_erc20_transfer() {
        let mut vm = BenchmarkingVm::new();
        let res = vm.run_transaction(&get_erc20_deploy_tx());
        assert_matches!(res.result, ExecutionResult::Success { .. });

        for nonce in 1..=5 {
            let res = vm.run_transaction(&get_erc20_transfer_tx(nonce));
            assert_matches!(res.result, ExecutionResult::Success { .. });
        }
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

    #[test]
    fn instruction_count_matches_on_both_vms_for_transfer() {
        let tx = get_transfer_tx(0);
        let legacy_count = Legacy::count_instructions(&tx);
        let fast_count = Fast::count_instructions(&tx);
        assert_eq!(legacy_count, fast_count);
    }

    #[test]
    fn instruction_count_matches_on_both_vms_for_benchmark_bytecodes() {
        for bytecode in BYTECODES {
            let tx = bytecode.deploy_tx();
            let legacy_count = Legacy::count_instructions(&tx);
            let fast_count = Fast::count_instructions(&tx);
            assert_eq!(legacy_count, fast_count, "bytecode: {}", bytecode.name);
        }
    }
}

//! Unit tests from the `testonly` test suite.

use std::{collections::HashSet, fmt, rc::Rc};

use zksync_types::{writes::StateDiffRecord, StorageKey, Transaction, H256, U256};
use zksync_vm2::interface::Tracer;
use zksync_vm_interface::{
    utils::{CheckDivergence, DivergenceErrors},
    Call,
};

use super::ShadowedFastVm;
use crate::{
    interface::{
        pubdata::{PubdataBuilder, PubdataInput},
        storage::InMemoryStorage,
        utils::{ShadowMut, ShadowRef},
        CurrentExecutionState, L2BlockEnv, VmExecutionResultAndLogs,
    },
    versions::testonly::{TestedVm, TestedVmWithCallTracer},
    vm_fast,
};

impl<Tr> TestedVm for ShadowedFastVm<InMemoryStorage, Tr>
where
    Tr: Tracer + Default + fmt::Debug + 'static,
{
    type StateDump = ();

    fn dump_state(&self) -> Self::StateDump {
        // Do nothing
    }

    fn gas_remaining(&mut self) -> u32 {
        self.get_mut("gas_remaining", |r| match r {
            ShadowMut::Main(vm) => vm.gas_remaining(),
            ShadowMut::Shadow(vm) => vm.gas_remaining(),
        })
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        self.get_custom("current_execution_state", |r| match r {
            ShadowRef::Main(vm) => vm.get_current_execution_state(),
            ShadowRef::Shadow(vm) => vm.get_current_execution_state(),
        })
    }

    fn decommitted_hashes(&self) -> HashSet<U256> {
        self.get("decommitted_hashes", |r| match r {
            ShadowRef::Main(vm) => vm.decommitted_hashes(),
            ShadowRef::Shadow(vm) => TestedVm::decommitted_hashes(vm),
        })
    }

    fn finish_batch_with_state_diffs(
        &mut self,
        diffs: Vec<StateDiffRecord>,
        pubdata_builder: Rc<dyn PubdataBuilder>,
    ) -> VmExecutionResultAndLogs {
        self.get_custom_mut("finish_batch_with_state_diffs", |r| match r {
            ShadowMut::Main(vm) => {
                vm.finish_batch_with_state_diffs(diffs.clone(), pubdata_builder.clone())
            }
            ShadowMut::Shadow(vm) => {
                vm.finish_batch_with_state_diffs(diffs.clone(), pubdata_builder.clone())
            }
        })
    }

    fn finish_batch_without_pubdata(&mut self) -> VmExecutionResultAndLogs {
        self.get_custom_mut("finish_batch_without_pubdata", |r| match r {
            ShadowMut::Main(vm) => vm.finish_batch_without_pubdata(),
            ShadowMut::Shadow(vm) => vm.finish_batch_without_pubdata(),
        })
    }

    fn insert_bytecodes(&mut self, bytecodes: &[&[u8]]) {
        self.get_mut("insert_bytecodes", |r| match r {
            ShadowMut::Main(vm) => vm.insert_bytecodes(bytecodes),
            ShadowMut::Shadow(vm) => TestedVm::insert_bytecodes(vm, bytecodes),
        });
    }

    fn known_bytecode_hashes(&self) -> HashSet<U256> {
        self.get("known_bytecode_hashes", |r| match r {
            ShadowRef::Main(vm) => vm.known_bytecode_hashes(),
            ShadowRef::Shadow(vm) => vm.known_bytecode_hashes(),
        })
    }

    fn manually_decommit(&mut self, code_hash: H256) -> bool {
        self.get_mut("manually_decommit", |r| match r {
            ShadowMut::Main(vm) => vm.manually_decommit(code_hash),
            ShadowMut::Shadow(vm) => vm.manually_decommit(code_hash),
        })
    }

    fn verify_required_bootloader_heap(&self, cells: &[(u32, U256)]) {
        self.get("verify_required_bootloader_heap", |r| match r {
            ShadowRef::Main(vm) => vm.verify_required_bootloader_heap(cells),
            ShadowRef::Shadow(vm) => vm.verify_required_bootloader_heap(cells),
        });
    }

    fn write_to_bootloader_heap(&mut self, cells: &[(usize, U256)]) {
        self.get_mut("manually_decommit", |r| match r {
            ShadowMut::Main(vm) => vm.write_to_bootloader_heap(cells),
            ShadowMut::Shadow(vm) => TestedVm::write_to_bootloader_heap(vm, cells),
        });
    }

    fn read_storage(&mut self, key: StorageKey) -> U256 {
        self.get_mut("read_storage", |r| match r {
            ShadowMut::Main(vm) => vm.read_storage(key),
            ShadowMut::Shadow(vm) => vm.read_storage(key),
        })
    }

    fn last_l2_block_hash(&self) -> H256 {
        self.get("last_l2_block_hash", |r| match r {
            ShadowRef::Main(vm) => vm.last_l2_block_hash(),
            ShadowRef::Shadow(vm) => vm.last_l2_block_hash(),
        })
    }

    fn push_l2_block_unchecked(&mut self, block: L2BlockEnv) {
        self.get_mut("push_l2_block_unchecked", |r| match r {
            ShadowMut::Main(vm) => vm.push_l2_block_unchecked(block),
            ShadowMut::Shadow(vm) => vm.push_l2_block_unchecked(block),
        });
    }

    fn push_transaction_with_refund(&mut self, tx: Transaction, refund: u64) {
        self.get_mut("push_transaction_with_refund", |r| match r {
            ShadowMut::Main(vm) => vm.push_transaction_with_refund(tx.clone(), refund),
            ShadowMut::Shadow(vm) => vm.push_transaction_with_refund(tx.clone(), refund),
        });
    }

    fn pubdata_input(&self) -> PubdataInput {
        self.get("pubdata_input", |r| match r {
            ShadowRef::Main(vm) => vm.pubdata_input(),
            ShadowRef::Shadow(vm) => vm.pubdata_input(),
        })
    }
}

#[derive(Debug)]
struct ExecutionResultAndTraces {
    result: VmExecutionResultAndLogs,
    traces: Vec<Call>,
}

impl From<(VmExecutionResultAndLogs, Vec<Call>)> for ExecutionResultAndTraces {
    fn from((result, traces): (VmExecutionResultAndLogs, Vec<Call>)) -> Self {
        Self { result, traces }
    }
}

impl From<ExecutionResultAndTraces> for (VmExecutionResultAndLogs, Vec<Call>) {
    fn from(value: ExecutionResultAndTraces) -> Self {
        (value.result, value.traces)
    }
}

impl CheckDivergence for ExecutionResultAndTraces {
    fn check_divergence(&self, other: &Self) -> DivergenceErrors {
        let mut errors = self.result.check_divergence(&other.result);
        errors.extend(self.traces.check_divergence(&other.traces));
        errors
    }
}

impl TestedVmWithCallTracer for ShadowedFastVm<InMemoryStorage, vm_fast::CallTracer> {
    fn inspect_with_call_tracer(&mut self) -> (VmExecutionResultAndLogs, Vec<Call>) {
        self.get_custom_mut("inspect_with_call_tracer", |r| {
            ExecutionResultAndTraces::from(match r {
                ShadowMut::Main(vm) => vm.inspect_with_call_tracer(),
                ShadowMut::Shadow(vm) => vm.inspect_with_call_tracer(),
            })
        })
        .into()
    }
}

mod block_tip {
    use crate::versions::testonly::block_tip::*;

    #[test]
    fn dry_run_upper_bound() {
        test_dry_run_upper_bound::<super::ShadowedFastVm>();
    }
}

mod bootloader {
    use crate::versions::testonly::bootloader::*;

    #[test]
    fn dummy_bootloader() {
        test_dummy_bootloader::<super::ShadowedFastVm>();
    }

    #[test]
    fn bootloader_out_of_gas() {
        test_bootloader_out_of_gas::<super::ShadowedFastVm>();
    }
}

mod bytecode_publishing {
    use crate::versions::testonly::bytecode_publishing::*;

    #[test]
    fn bytecode_publishing() {
        test_bytecode_publishing::<super::ShadowedFastVm>();
    }
}

mod call_tracer {
    use crate::versions::testonly::call_tracer::*;

    #[test]
    fn basic_behavior() {
        test_basic_behavior::<super::ShadowedFastVm<_, _>>();
    }

    #[test]
    fn transfer() {
        test_transfer::<super::ShadowedFastVm<_, _>>();
    }

    #[test]
    fn reverted_tx() {
        test_reverted_tx::<super::ShadowedFastVm<_, _>>();
    }

    #[test]
    fn reverted_deployment() {
        test_reverted_deployment_tx::<super::ShadowedFastVm<_, _>>();
    }

    #[test]
    fn out_of_gas() {
        test_out_of_gas::<super::ShadowedFastVm<_, _>>();
    }

    #[test]
    fn recursive_tx() {
        test_recursive_tx::<super::ShadowedFastVm<_, _>>();
    }

    #[test]
    fn evm_to_eravm_call() {
        test_evm_to_eravm_call::<super::ShadowedFastVm<_, _>>();
    }

    #[test]
    fn evm_deployment_tx() {
        test_evm_deployment_tx::<super::ShadowedFastVm<_, _>>();
    }

    #[test]
    fn evm_deployment_from_contract() {
        test_evm_deployment_from_contract::<super::ShadowedFastVm<_, _>>();
    }
}

mod circuits {
    use crate::versions::testonly::circuits::*;

    #[test]
    fn circuits() {
        test_circuits::<super::ShadowedFastVm>();
    }
}

mod code_oracle {
    use crate::versions::testonly::code_oracle::*;

    #[test]
    fn code_oracle() {
        test_code_oracle::<super::ShadowedFastVm>();
    }

    #[test]
    fn code_oracle_big_bytecode() {
        test_code_oracle_big_bytecode::<super::ShadowedFastVm>();
    }

    #[test]
    fn refunds_in_code_oracle() {
        test_refunds_in_code_oracle::<super::ShadowedFastVm>();
    }
}

mod default_aa {
    use crate::versions::testonly::default_aa::*;

    #[test]
    fn default_aa_interaction() {
        test_default_aa_interaction::<super::ShadowedFastVm>();
    }

    #[test]
    fn permissive_aa_works() {
        test_permissive_aa_works::<super::ShadowedFastVm>();
    }
}

mod evm {
    use crate::versions::testonly::evm::*;

    #[test]
    fn evm_deployment_tx() {
        test_evm_deployment_tx::<super::ShadowedFastVm>();
    }

    #[test]
    fn evm_bytecode_decommit() {
        test_evm_bytecode_decommit::<super::ShadowedFastVm>();
    }

    #[test]
    fn real_emulator_basics() {
        test_real_emulator_basics::<super::ShadowedFastVm>();
    }

    #[test]
    fn real_emulator_code_hash() {
        test_real_emulator_code_hash::<super::ShadowedFastVm>();
    }

    #[test]
    fn real_emulator_block_info() {
        test_real_emulator_block_info::<super::ShadowedFastVm>();
    }

    #[test]
    fn real_emulator_msg_info() {
        test_real_emulator_msg_info::<super::ShadowedFastVm>();
    }

    #[test]
    fn real_emulator_gas_management() {
        test_real_emulator_gas_management::<super::ShadowedFastVm>();
    }

    #[test]
    fn real_emulator_recursion() {
        test_real_emulator_recursion::<super::ShadowedFastVm>();
    }

    #[test]
    fn real_emulator_deployment() {
        test_real_emulator_deployment::<super::ShadowedFastVm>();
    }

    #[test]
    fn real_emulator_create2_deployment() {
        test_create2_deployment_in_evm::<super::ShadowedFastVm>();
    }

    #[test]
    fn reusing_create_address() {
        test_reusing_create_address_in_evm::<super::ShadowedFastVm>();
    }

    #[test]
    fn reusing_create2_salt() {
        test_reusing_create2_salt_in_evm::<super::ShadowedFastVm>();
    }

    #[test]
    fn deployment_with_partial_reverts() {
        test_deployment_with_partial_reverts::<super::ShadowedFastVm>();
    }

    #[test]
    fn era_vm_deployment_after_evm_execution() {
        test_era_vm_deployment_after_evm_execution::<super::ShadowedFastVm>();
    }

    #[test]
    fn era_vm_deployment_after_evm_deployment() {
        test_era_vm_deployment_after_evm_deployment::<super::ShadowedFastVm>();
    }

    #[test]
    fn calling_era_contract_from_evm() {
        test_calling_era_contract_from_evm::<super::ShadowedFastVm>();
    }

    #[test]
    fn far_calls_from_evm_contract() {
        test_far_calls_from_evm_contract::<super::ShadowedFastVm>();
    }

    #[test]
    fn calling_sha256_precompile() {
        test_calling_sha256_precompile::<super::ShadowedFastVm>();
    }

    #[test]
    fn calling_ecrecover_precompile() {
        test_calling_ecrecover_precompile::<super::ShadowedFastVm>();
    }
}

mod mock_evm {
    use test_casing::{test_casing, Product};

    use crate::versions::testonly::mock_evm::*;

    #[test]
    fn tracing_evm_contract_deployment() {
        test_tracing_evm_contract_deployment::<super::ShadowedFastVm>();
    }

    #[test]
    fn mock_emulator_basics() {
        test_mock_emulator_basics::<super::ShadowedFastVm>();
    }

    #[test_casing(2, [false, true])]
    #[test]
    fn mock_emulator_with_payment(deploy_emulator: bool) {
        test_mock_emulator_with_payment::<super::ShadowedFastVm>(deploy_emulator);
    }

    #[test_casing(4, Product(([false, true], [false, true])))]
    #[test]
    fn mock_emulator_with_recursion(deploy_emulator: bool, is_external: bool) {
        test_mock_emulator_with_recursion::<super::ShadowedFastVm>(deploy_emulator, is_external);
    }

    #[test]
    fn calling_to_mock_emulator_from_native_contract() {
        test_calling_to_mock_emulator_from_native_contract::<super::ShadowedFastVm>();
    }

    #[test]
    fn mock_emulator_with_deployment() {
        test_mock_emulator_with_deployment::<super::ShadowedFastVm>(false);
    }

    #[test]
    fn mock_emulator_with_reverted_deployment() {
        test_mock_emulator_with_deployment::<super::ShadowedFastVm>(true);
    }

    #[test]
    fn mock_emulator_with_recursive_deployment() {
        test_mock_emulator_with_recursive_deployment::<super::ShadowedFastVm>();
    }

    #[test]
    fn mock_emulator_with_partial_reverts() {
        test_mock_emulator_with_partial_reverts::<super::ShadowedFastVm>();
    }

    #[test]
    fn mock_emulator_with_delegate_call() {
        test_mock_emulator_with_delegate_call::<super::ShadowedFastVm>();
    }

    #[test]
    fn mock_emulator_with_static_call() {
        test_mock_emulator_with_static_call::<super::ShadowedFastVm>();
    }
}

mod gas_limit {
    use crate::versions::testonly::gas_limit::*;

    #[test]
    fn tx_gas_limit_offset() {
        test_tx_gas_limit_offset::<super::ShadowedFastVm>();
    }
}

mod get_used_contracts {
    use crate::versions::testonly::get_used_contracts::*;

    #[test]
    fn get_used_contracts() {
        test_get_used_contracts::<super::ShadowedFastVm>();
    }

    #[test]
    fn get_used_contracts_with_far_call() {
        test_get_used_contracts_with_far_call::<super::ShadowedFastVm>();
    }

    #[test]
    fn get_used_contracts_with_out_of_gas_far_call() {
        test_get_used_contracts_with_out_of_gas_far_call::<super::ShadowedFastVm>();
    }
}

mod is_write_initial {
    use crate::versions::testonly::is_write_initial::*;

    #[test]
    fn is_write_initial_behaviour() {
        test_is_write_initial_behaviour::<super::ShadowedFastVm>();
    }
}

mod l1_messenger {
    use crate::versions::testonly::l1_messenger::*;

    #[test]
    fn rollup_da_output_hash_match() {
        test_rollup_da_output_hash_match::<super::ShadowedFastVm>();
    }
}

mod l1_tx_execution {
    use crate::versions::testonly::l1_tx_execution::*;

    #[test]
    fn l1_tx_execution() {
        test_l1_tx_execution::<super::ShadowedFastVm>();
    }

    #[test]
    fn l1_tx_execution_high_gas_limit() {
        test_l1_tx_execution_high_gas_limit::<super::ShadowedFastVm>();
    }
}

mod l2_blocks {
    use crate::versions::testonly::l2_blocks::*;

    #[test]
    fn l2_block_initialization_timestamp() {
        test_l2_block_initialization_timestamp::<super::ShadowedFastVm>();
    }

    #[test]
    fn l2_block_initialization_number_non_zero() {
        test_l2_block_initialization_number_non_zero::<super::ShadowedFastVm>();
    }

    #[test]
    fn l2_block_same_l2_block() {
        test_l2_block_same_l2_block::<super::ShadowedFastVm>();
    }

    #[test]
    fn l2_block_new_l2_block() {
        test_l2_block_new_l2_block::<super::ShadowedFastVm>();
    }

    #[test]
    fn l2_block_first_in_batch() {
        test_l2_block_first_in_batch::<super::ShadowedFastVm>();
    }
}

mod nonce_holder {
    use crate::versions::testonly::nonce_holder::*;

    #[test]
    fn nonce_holder() {
        test_nonce_holder::<super::ShadowedFastVm>();
    }
}

mod precompiles {
    use crate::versions::testonly::precompiles::*;

    #[test]
    fn keccak() {
        test_keccak::<super::ShadowedFastVm>();
    }

    #[test]
    fn sha256() {
        test_sha256::<super::ShadowedFastVm>();
    }

    #[test]
    fn ecrecover() {
        test_ecrecover::<super::ShadowedFastVm>();
    }

    #[test]
    fn ecadd() {
        test_ecadd::<super::ShadowedFastVm>();
    }

    #[test]
    fn ecmul() {
        test_ecmul::<super::ShadowedFastVm>();
    }

    #[test]
    fn ecpairing() {
        test_ecpairing::<super::ShadowedFastVm>();
    }

    #[test]
    fn modexp() {
        test_modexp::<super::ShadowedFastVm>();
    }

    #[test]
    fn v28_precompiles_disabled() {
        test_v28_precompiles_disabled::<super::ShadowedFastVm>();
    }
}

mod refunds {
    use crate::versions::testonly::refunds::*;

    #[test]
    fn predetermined_refunded_gas() {
        test_predetermined_refunded_gas::<super::ShadowedFastVm>();
    }

    #[test]
    fn negative_pubdata_for_transaction() {
        test_negative_pubdata_for_transaction::<super::ShadowedFastVm>();
    }
}

mod require_eip712 {
    use crate::versions::testonly::require_eip712::*;

    #[test]
    fn require_eip712() {
        test_require_eip712::<super::ShadowedFastVm>();
    }
}

mod rollbacks {
    use crate::versions::testonly::rollbacks::*;

    #[test]
    fn vm_rollbacks() {
        test_vm_rollbacks::<super::ShadowedFastVm>();
    }

    #[test]
    fn vm_loadnext_rollbacks() {
        test_vm_loadnext_rollbacks::<super::ShadowedFastVm>();
    }

    #[test]
    fn rollback_in_call_mode() {
        test_rollback_in_call_mode::<super::ShadowedFastVm>();
    }
}

mod secp256r1 {
    use crate::versions::testonly::secp256r1::*;

    #[test]
    fn secp256r1() {
        test_secp256r1::<super::ShadowedFastVm>();
    }
}

mod simple_execution {
    use crate::versions::testonly::simple_execution::*;

    #[test]
    fn estimate_fee() {
        test_estimate_fee::<super::ShadowedFastVm>();
    }

    #[test]
    fn simple_execute() {
        test_simple_execute::<super::ShadowedFastVm>();
    }

    #[test]
    fn create2_deployment_address() {
        test_create2_deployment_address::<super::ShadowedFastVm>();
    }

    #[test]
    fn reusing_create_address() {
        test_reusing_create_address::<super::ShadowedFastVm>();
    }

    #[test]
    fn reusing_create2_salt() {
        test_reusing_create2_salt::<super::ShadowedFastVm>();
    }

    #[test]
    fn transfer_to_self_with_low_gas_limit() {
        test_transfer_to_self_with_low_gas_limit::<super::ShadowedFastVm<_>>();
    }
}

mod storage {
    use crate::versions::testonly::storage::*;

    #[test]
    fn storage_behavior() {
        test_storage_behavior::<super::ShadowedFastVm>();
    }

    #[test]
    fn transient_storage_behavior() {
        test_transient_storage_behavior::<super::ShadowedFastVm>();
    }
}

mod tracing_execution_error {
    use crate::versions::testonly::tracing_execution_error::*;

    #[test]
    fn tracing_of_execution_errors() {
        test_tracing_of_execution_errors::<super::ShadowedFastVm>();
    }
}

mod transfer {
    use crate::versions::testonly::transfer::*;

    #[test]
    fn send_and_transfer() {
        test_send_and_transfer::<super::ShadowedFastVm>();
    }

    #[test]
    fn reentrancy_protection_send_and_transfer() {
        test_reentrancy_protection_send_and_transfer::<super::ShadowedFastVm>();
    }
}

mod upgrade {
    use crate::versions::testonly::upgrade::*;

    #[test]
    fn protocol_upgrade_is_first() {
        test_protocol_upgrade_is_first::<super::ShadowedFastVm>();
    }

    #[test]
    fn force_deploy_upgrade() {
        test_force_deploy_upgrade::<super::ShadowedFastVm>();
    }

    #[test]
    fn complex_upgrader() {
        test_complex_upgrader::<super::ShadowedFastVm>();
    }
}

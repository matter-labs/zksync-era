use std::collections::HashSet;

use zk_evm_1_5_0::{
    aux_structures::{MemoryPage, Timestamp},
    zkevm_opcode_defs::{ContractCodeSha256Format, VersionedHashLen32},
};
use zksync_types::{writes::StateDiffRecord, StorageKey, Transaction, H256, U256};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256};
use zksync_vm_interface::{
    CurrentExecutionState, L2BlockEnv, VmExecutionMode, VmExecutionResultAndLogs,
};

use super::{HistoryEnabled, Vm};
use crate::{
    interface::storage::{InMemoryStorage, StorageView},
    versions::testonly::TestedVm,
    vm_latest::{
        constants::BOOTLOADER_HEAP_PAGE, tracers::PubdataTracer, types::internals::TransactionData,
        TracerDispatcher,
    },
};

mod bootloader;
mod default_aa;
// TODO - fix this test
// `mod invalid_bytecode;`
mod block_tip;
mod bytecode_publishing;
mod call_tracer;
mod circuits;
mod code_oracle;
mod constants;
mod evm_emulator;
mod gas_limit;
mod get_used_contracts;
mod is_write_initial;
mod l1_tx_execution;
mod l2_blocks;
mod migration;
mod nonce_holder;
mod precompiles;
mod prestate_tracer;
mod refunds;
mod require_eip712;
mod rollbacks;
mod secp256r1;
mod simple_execution;
mod storage;
mod tester;
mod tracing_execution_error;
mod transfer;
mod upgrade;
mod utils;

impl TestedVm for Vm<StorageView<InMemoryStorage>, HistoryEnabled> {
    fn gas_remaining(&mut self) -> u32 {
        self.state.local_state.callstack.current.ergs_remaining
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        self.get_current_execution_state()
    }

    fn decommitted_hashes(&self) -> HashSet<U256> {
        self.get_used_contracts().into_iter().collect()
    }

    fn execute_with_state_diffs(
        &mut self,
        diffs: Vec<StateDiffRecord>,
        mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        let pubdata_tracer = PubdataTracer::new_with_forced_state_diffs(
            self.batch_env.clone(),
            VmExecutionMode::Batch,
            diffs,
            crate::vm_latest::MultiVMSubversion::latest(),
        );
        self.inspect_inner(&mut TracerDispatcher::default(), mode, Some(pubdata_tracer))
    }

    fn insert_bytecodes(&mut self, bytecodes: &[&[u8]]) {
        let bytecodes = bytecodes
            .iter()
            .map(|&bytecode| {
                let hash = hash_bytecode(bytecode);
                let words = bytes_to_be_words(bytecode.to_vec());
                (h256_to_u256(hash), words)
            })
            .collect();
        self.state
            .decommittment_processor
            .populate(bytecodes, Timestamp(0));
    }

    fn known_bytecode_hashes(&self) -> HashSet<U256> {
        self.state
            .decommittment_processor
            .known_bytecodes
            .inner()
            .keys()
            .copied()
            .collect()
    }

    fn manually_decommit(&mut self, code_hash: H256) -> bool {
        let (header, normalized_preimage) =
            ContractCodeSha256Format::normalize_for_decommitment(&code_hash.0);
        let query = self
            .state
            .prepare_to_decommit(
                0,
                header,
                normalized_preimage,
                MemoryPage(123),
                Timestamp(0),
            )
            .unwrap();
        self.state.execute_decommit(0, query).unwrap();
        query.is_fresh
    }

    fn verify_required_bootloader_heap(&self, cells: &[(u32, U256)]) {
        for &(slot, required_value) in cells {
            let current_value = self
                .state
                .memory
                .read_slot(BOOTLOADER_HEAP_PAGE as usize, slot as usize)
                .value;
            assert_eq!(current_value, required_value);
        }
    }

    fn write_to_bootloader_heap(&mut self, cells: &[(usize, U256)]) {
        let timestamp = Timestamp(self.state.local_state.timestamp);
        self.state
            .memory
            .populate_page(BOOTLOADER_HEAP_PAGE as usize, cells.to_vec(), timestamp)
    }

    fn read_storage(&mut self, key: StorageKey) -> U256 {
        self.state.storage.storage.read_from_storage(&key)
    }

    fn last_l2_block_hash(&self) -> H256 {
        self.bootloader_state.last_l2_block().get_hash()
    }

    fn push_l2_block_unchecked(&mut self, block: L2BlockEnv) {
        self.bootloader_state.push_l2_block(block);
    }

    fn push_transaction_with_refund(&mut self, tx: Transaction, refund: u64) {
        let tx = TransactionData::new(tx, false);
        let overhead = tx.overhead_gas();
        self.push_raw_transaction(tx, overhead, refund, true)
    }
}

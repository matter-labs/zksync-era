use zk_evm_1_4_1::aux_structures::Timestamp;
use zksync_state::WriteStorage;
use zksync_types::{l1::is_l1_tx_type, Transaction};

use crate::{
    vm_latest::{
        constants::BOOTLOADER_HEAP_PAGE,
        implementation::bytecode::{bytecode_to_factory_dep, compress_bytecodes},
        types::internals::TransactionData,
        vm::Vm,
    },
    HistoryMode,
};

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    pub(crate) fn push_raw_transaction(
        &mut self,
        tx: TransactionData,
        predefined_overhead: u32,
        predefined_refund: u32,
        with_compression: bool,
    ) {
        let timestamp = Timestamp(self.state.local_state.timestamp);
        let codes_for_decommiter = tx
            .factory_deps
            .iter()
            .map(|dep| bytecode_to_factory_dep(dep.clone()))
            .collect();

        let compressed_bytecodes = if is_l1_tx_type(tx.tx_type) || !with_compression {
            // L1 transactions do not need compression
            vec![]
        } else {
            compress_bytecodes(&tx.factory_deps, self.state.storage.storage.get_ptr())
        };

        self.state
            .decommittment_processor
            .populate(codes_for_decommiter, timestamp);

        let trusted_ergs_limit = tx.trusted_ergs_limit();

        let memory = self.bootloader_state.push_tx(
            tx,
            predefined_overhead,
            predefined_refund,
            compressed_bytecodes,
            trusted_ergs_limit,
            self.system_env.chain_id,
        );

        self.state
            .memory
            .populate_page(BOOTLOADER_HEAP_PAGE as usize, memory, timestamp);
    }

    pub(crate) fn push_transaction_with_compression(
        &mut self,
        tx: Transaction,
        with_compression: bool,
    ) {
        let tx: TransactionData = tx.into();
        let overhead = tx.overhead_gas();
        self.push_raw_transaction(tx, overhead, 0, with_compression);
    }
}

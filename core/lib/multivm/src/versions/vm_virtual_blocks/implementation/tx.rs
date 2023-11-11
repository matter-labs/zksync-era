use crate::vm_virtual_blocks::constants::BOOTLOADER_HEAP_PAGE;
use crate::vm_virtual_blocks::implementation::bytecode::{
    bytecode_to_factory_dep, compress_bytecodes,
};
use crate::HistoryMode;
use zk_evm_1_3_3::aux_structures::Timestamp;
use zksync_state::WriteStorage;
use zksync_types::l1::is_l1_tx_type;
use zksync_types::Transaction;

use crate::vm_virtual_blocks::types::internals::TransactionData;
use crate::vm_virtual_blocks::vm::Vm;

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

        let trusted_ergs_limit =
            tx.trusted_ergs_limit(self.batch_env.block_gas_price_per_pubdata());

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
        let block_gas_per_pubdata_byte = self.batch_env.block_gas_price_per_pubdata();
        let overhead = tx.overhead_gas(block_gas_per_pubdata_byte as u32);
        self.push_raw_transaction(tx, overhead, 0, with_compression);
    }
}

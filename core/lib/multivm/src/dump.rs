use crate::{
    interface::{
        storage::{StoragePtr, StorageSnapshot, StorageView},
        utils::VmDump,
        L1BatchEnv, L2BlockEnv, SystemEnv, VmFactory, VmInterface, VmInterfaceExt,
    },
    pubdata_builders::pubdata_params_to_builder,
};

/// Plays back dump on the specified VM.
pub fn play_back_dump<Vm>(dump: VmDump) -> Vm
where
    Vm: VmFactory<StorageView<StorageSnapshot>>,
{
    play_back_dump_custom(dump, Vm::new)
}

/// Plays back dump on a VM created using the provided closure.
#[doc(hidden)] // too low-level
pub fn play_back_dump_custom<Vm: VmInterface>(
    dump: VmDump,
    create_vm: impl FnOnce(L1BatchEnv, SystemEnv, StoragePtr<StorageView<StorageSnapshot>>) -> Vm,
) -> Vm {
    let storage = StorageView::new(dump.storage).to_rc_ptr();
    let mut vm = create_vm(dump.l1_batch_env, dump.system_env, storage);

    for (i, l2_block) in dump.l2_blocks.into_iter().enumerate() {
        if i > 0 {
            // First block is already set.
            vm.start_new_l2_block(L2BlockEnv {
                number: l2_block.number.0,
                timestamp: l2_block.timestamp,
                prev_block_hash: l2_block.prev_block_hash,
                max_virtual_blocks_to_create: l2_block.virtual_blocks,
            });
        }

        for tx in l2_block.txs {
            let tx_hash = tx.hash();
            let (compression_result, _) =
                vm.execute_transaction_with_bytecode_compression(tx, true);
            if let Err(err) = compression_result {
                panic!("Failed compressing bytecodes for transaction {tx_hash:?}: {err}");
            }
        }
    }
    vm.finish_batch(dump.pubdata_params.map(pubdata_params_to_builder));
    vm
}

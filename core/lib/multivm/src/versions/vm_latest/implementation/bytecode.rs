use itertools::Itertools;
use zksync_types::U256;
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words};

use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        CompressedBytecodeInfo,
    },
    utils::bytecode,
    vm_latest::Vm,
    HistoryMode,
};

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    /// Checks the last transaction has successfully published compressed bytecodes and returns `true` if there is at least one is still unknown.
    pub(crate) fn has_unpublished_bytecodes(&mut self) -> bool {
        self.bootloader_state
            .get_last_tx_compressed_bytecodes()
            .iter()
            .any(|info| {
                !self
                    .state
                    .storage
                    .storage
                    .get_ptr()
                    .borrow_mut()
                    .is_bytecode_known(&hash_bytecode(&info.original))
            })
    }
}

/// Converts bytecode to tokens and hashes it.
pub(crate) fn bytecode_to_factory_dep(bytecode: Vec<u8>) -> (U256, Vec<U256>) {
    let bytecode_hash = hash_bytecode(&bytecode);
    let bytecode_hash = U256::from_big_endian(bytecode_hash.as_bytes());

    let bytecode_words = bytes_to_be_words(bytecode);

    (bytecode_hash, bytecode_words)
}

pub(crate) fn compress_bytecodes<S: WriteStorage>(
    bytecodes: &[Vec<u8>],
    storage: StoragePtr<S>,
) -> Vec<CompressedBytecodeInfo> {
    bytecodes
        .iter()
        .enumerate()
        .sorted_by_key(|(_idx, dep)| *dep)
        .dedup_by(|x, y| x.1 == y.1)
        .filter(|(_idx, dep)| !storage.borrow_mut().is_bytecode_known(&hash_bytecode(dep)))
        .sorted_by_key(|(idx, _dep)| *idx)
        .filter_map(|(_idx, dep)| bytecode::compress(dep.clone()).ok())
        .collect()
}

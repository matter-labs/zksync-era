use itertools::Itertools;
use zksync_types::H256;
use zksync_utils::bytecode::{compress_bytecode, hash_bytecode, CompressedBytecodeInfo};

/*impl Vm {
    /// Checks the last transaction has successfully published compressed bytecodes and returns `true` if there is at least one is still unknown.
    pub(crate) fn has_unpublished_bytecodes(&mut self) -> bool {
        self.get_last_tx_compressed_bytecodes().iter().any(|info| {
            !self
                .state
                .storage
                .storage
                .get_ptr()
                .borrow_mut()
                .is_bytecode_known(&hash_bytecode(&info.original))
        })
    }
}*/

pub(crate) fn compress_bytecodes(
    bytecodes: &[Vec<u8>],
    is_bytecode_known: impl Fn(H256) -> bool,
) -> Vec<CompressedBytecodeInfo> {
    bytecodes
        .iter()
        .enumerate()
        .sorted_by_key(|(_idx, dep)| *dep)
        .dedup_by(|x, y| x.1 == y.1)
        .filter(|(_idx, dep)| !is_bytecode_known(hash_bytecode(dep)))
        .sorted_by_key(|(idx, _dep)| *idx)
        .filter_map(|(_idx, dep)| {
            let compressed_bytecode = compress_bytecode(dep);

            compressed_bytecode
                .ok()
                .map(|compressed| CompressedBytecodeInfo {
                    original: dep.clone(),
                    compressed,
                })
        })
        .collect()
}

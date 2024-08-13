use itertools::Itertools;
use zksync_types::H256;
use zksync_utils::{
    bytecode::{compress_bytecode, hash_bytecode, CompressedBytecodeInfo},
    h256_to_u256,
};

use super::Vm;
use crate::interface::storage::ReadStorage;

impl<S: ReadStorage> Vm<S> {
    /// Checks the last transaction has successfully published compressed bytecodes and returns `true` if there is at least one is still unknown.
    pub(crate) fn has_unpublished_bytecodes(&mut self) -> bool {
        self.bootloader_state
            .get_last_tx_compressed_bytecodes()
            .iter()
            .any(|info| {
                let hash_bytecode = hash_bytecode(&info.original);
                let is_bytecode_known = self.world.storage.is_bytecode_known(&hash_bytecode);

                let is_bytecode_known_cache = self
                    .world
                    .bytecode_cache
                    .contains_key(&h256_to_u256(hash_bytecode));
                !(is_bytecode_known || is_bytecode_known_cache)
            })
    }
}

pub(crate) fn compress_bytecodes(
    bytecodes: &[Vec<u8>],
    mut is_bytecode_known: impl FnMut(H256) -> bool,
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

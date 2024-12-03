use itertools::Itertools;
use zksync_types::{bytecode::BytecodeHash, h256_to_u256, H256};

use super::Vm;
use crate::{
    interface::{storage::ReadStorage, CompressedBytecodeInfo},
    utils::bytecode,
};

impl<S: ReadStorage, Tr, Val> Vm<S, Tr, Val> {
    /// Checks the last transaction has successfully published compressed bytecodes and returns `true` if there is at least one is still unknown.
    pub(crate) fn has_unpublished_bytecodes(&mut self) -> bool {
        self.bootloader_state
            .get_last_tx_compressed_bytecodes()
            .iter()
            .any(|info| {
                let hash_bytecode = BytecodeHash::for_bytecode(&info.original).value();
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
        .filter(|(_idx, dep)| !is_bytecode_known(BytecodeHash::for_bytecode(dep).value()))
        .sorted_by_key(|(idx, _dep)| *idx)
        .filter_map(|(_idx, dep)| bytecode::compress(dep.clone()).ok())
        .collect()
}

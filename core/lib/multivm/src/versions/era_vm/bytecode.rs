use itertools::Itertools;
use zksync_types::H256;
use zksync_utils::bytecode::{compress_bytecode, hash_bytecode, CompressedBytecodeInfo};

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

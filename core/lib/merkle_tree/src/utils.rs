use crate::types::LevelIndex;
use zksync_config::constants::ROOT_TREE_DEPTH;
use zksync_types::U256;

/// Calculates neighbor index for given index to have complete pair.
fn neighbor_idx(idx: U256) -> U256 {
    idx ^ 1.into()
}

/// Produces a full merkle path of neighbors for given leaf (including given leaf itself)
/// Used to calculate hash changes for branch nodes caused by leaf update
pub fn idx_to_merkle_path(idx: U256) -> impl DoubleEndedIterator<Item = LevelIndex> + Clone {
    (1..=ROOT_TREE_DEPTH)
        .map(move |cur_depth| {
            (
                cur_depth as u16,
                neighbor_idx(idx >> (ROOT_TREE_DEPTH - cur_depth)),
            )
        })
        .map(Into::into)
}

pub(crate) fn children_idxs(level_idx: &LevelIndex) -> (LevelIndex, LevelIndex) {
    (
        (level_idx.0 .0 + 1, level_idx.0 .1 << 1).into(),
        (level_idx.0 .0 + 1, (level_idx.0 .1 << 1) + 1).into(),
    )
}

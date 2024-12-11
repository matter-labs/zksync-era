/// Position of `chainCount` in `MessageRoot`'s storage layout.
pub const CHAIN_COUNT_KEY: usize = 0;

/// Position of `chainIndexToId` in `MessageRoot`'s storage layout.
pub const CHAIN_INDEX_TO_ID_KEY: usize = 2;

/// Position of `FullTree::_height` in `MessageRoot`'s storage layout.
pub const AGG_TREE_HEIGHT_KEY: usize = 3;

/// Position of `FullTree::nodes` in `MessageRoot`'s storage layout.
pub const AGG_TREE_NODES_KEY: usize = 5;

/// Position of `chainTree` in `MessageRoot`'s storage layout.
pub const CHAIN_TREE_KEY: usize = 7;

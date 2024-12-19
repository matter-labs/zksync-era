pub const ZKPORTER_IS_AVAILABLE: bool = false;

/// Depth of the account tree.
pub const ROOT_TREE_DEPTH: usize = 256;

/// To avoid DDoS we limit the size of the transactions size.
/// TODO(X): remove this as a constant and introduce a config.
pub const MAX_ENCODED_TX_SIZE: usize = 1 << 24;

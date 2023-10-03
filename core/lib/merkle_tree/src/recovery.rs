//! Merkle tree recovery logic.

use crate::types::{Key, ValueHash};

#[derive(Debug, Clone)]
pub struct RecoveryEntry {
    pub key: Key,
    pub value: ValueHash,
    pub leaf_index: u64,
    // FIXME: add version
}

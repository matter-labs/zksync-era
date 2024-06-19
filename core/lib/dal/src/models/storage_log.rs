use zksync_types::{L1BatchNumber, L2BlockNumber, H160, H256, U256};

/// Model of the initial write record from the `initial_writes` table. Should only be used in tests.
#[derive(Debug, PartialEq)]
pub struct DbInitialWrite {
    pub hashed_key: H256,
    pub l1_batch_number: L1BatchNumber,
    pub index: u64,
}

/// Model of the storage log record from the `storage_logs` table. Should only be used in tests.
#[derive(Debug, PartialEq)]
pub struct DbStorageLog {
    pub hashed_key: H256,
    pub address: H160,
    pub key: H256,
    pub value: H256,
    pub operation_number: u64,
    pub l2_block_number: L2BlockNumber,
}

// We don't want to rely on the Merkle tree crate to import a single type, so we duplicate `TreeEntry` here.
#[derive(Debug, Clone, Copy)]
pub struct StorageRecoveryLogEntry {
    pub key: H256,
    pub value: H256,
    pub leaf_index: u64,
}

impl StorageRecoveryLogEntry {
    /// Converts `key` to the format used by the Merkle tree (little-endian [`U256`]).
    pub fn tree_key(&self) -> U256 {
        U256::from_little_endian(&self.key.0)
    }
}

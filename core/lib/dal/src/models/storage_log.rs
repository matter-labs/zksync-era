use sqlx::types::chrono::NaiveDateTime;
use zksync_types::{AccountTreeId, Address, StorageKey, StorageLog, StorageLogKind, H256, U256};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DBStorageLog {
    pub id: i64,
    pub hashed_key: Vec<u8>,
    pub address: Vec<u8>,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub operation_number: i32,
    pub tx_hash: Vec<u8>,
    pub miniblock_number: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl From<DBStorageLog> for StorageLog {
    fn from(log: DBStorageLog) -> StorageLog {
        StorageLog {
            kind: StorageLogKind::Write,
            key: StorageKey::new(
                AccountTreeId::new(Address::from_slice(&log.address)),
                H256::from_slice(&log.key),
            ),
            value: H256::from_slice(&log.value),
        }
    }
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

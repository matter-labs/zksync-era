use sqlx::types::chrono::NaiveDateTime;
use zksync_types::{AccountTreeId, Address, StorageKey, StorageLog, StorageLogKind, H256};

#[derive(sqlx::FromRow, Debug, Clone)]
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

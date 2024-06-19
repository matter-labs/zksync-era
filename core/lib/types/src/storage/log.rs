use std::mem;

use serde::{Deserialize, Serialize};
use zksync_basic_types::AccountTreeId;
use zksync_utils::{h256_to_u256, u256_to_h256};

use crate::{
    api::ApiStorageLog,
    zk_evm_types::{self, LogQuery, Timestamp},
    StorageKey, StorageValue, U256,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageLogKind {
    Read,
    InitialWrite,
    RepeatedWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StorageLog {
    pub kind: StorageLogKind,
    pub key: StorageKey,
    pub value: StorageValue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StorageLogWithPreviousValue {
    pub log: StorageLog,
    pub previous_value: StorageValue,
}

impl StorageLog {
    pub fn from_log_query(log: &LogQuery) -> Self {
        let key = StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key));
        if log.rw_flag {
            if log.rollback {
                Self::new_write_log(key, u256_to_h256(log.read_value))
            } else {
                Self::new_write_log(key, u256_to_h256(log.written_value))
            }
        } else {
            Self::new_read_log(key, u256_to_h256(log.read_value))
        }
    }

    pub fn new_read_log(key: StorageKey, value: StorageValue) -> Self {
        Self {
            kind: StorageLogKind::Read,
            key,
            value,
        }
    }

    pub fn new_write_log(key: StorageKey, value: StorageValue) -> Self {
        Self {
            kind: StorageLogKind::RepeatedWrite,
            key,
            value,
        }
    }

    pub fn is_write(&self) -> bool {
        !matches!(self.kind, StorageLogKind::Read)
    }

    /// Converts this log to a log query that could be used in tests.
    pub fn to_test_log_query(&self) -> LogQuery {
        let mut read_value = U256::zero();
        let mut written_value = U256::from_big_endian(self.value.as_bytes());
        if self.kind == StorageLogKind::Read {
            mem::swap(&mut read_value, &mut written_value);
        }

        LogQuery {
            timestamp: Timestamp(0),
            tx_number_in_block: 0,
            aux_byte: 0,
            shard_id: 0,
            address: *self.key.address(),
            key: U256::from_big_endian(self.key.key().as_bytes()),
            read_value,
            written_value,
            rw_flag: self.is_write(),
            rollback: false,
            is_service: false,
        }
    }
}

impl From<zk_evm_types::LogQuery> for StorageLog {
    fn from(log_query: zk_evm_types::LogQuery) -> Self {
        Self::from_log_query(&log_query)
    }
}

impl From<StorageLog> for ApiStorageLog {
    fn from(storage_log: StorageLog) -> Self {
        Self {
            address: *storage_log.key.address(),
            key: h256_to_u256(*storage_log.key.key()),
            written_value: h256_to_u256(storage_log.value),
        }
    }
}

impl From<&StorageLogWithPreviousValue> for ApiStorageLog {
    fn from(log: &StorageLogWithPreviousValue) -> Self {
        log.log.into()
    }
}

/// Log query, which handle initial and repeated writes to the storage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageLogQuery {
    pub log_query: LogQuery,
    pub log_type: StorageLogKind,
}

impl From<&StorageLogQuery> for ApiStorageLog {
    fn from(log_query: &StorageLogQuery) -> Self {
        log_query.log_query.into()
    }
}

impl From<LogQuery> for ApiStorageLog {
    fn from(log_query: LogQuery) -> Self {
        ApiStorageLog {
            address: log_query.address,
            key: log_query.key,
            written_value: log_query.written_value,
        }
    }
}

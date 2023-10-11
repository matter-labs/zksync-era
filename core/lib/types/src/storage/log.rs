use serde::{Deserialize, Serialize};

use std::mem;

use zk_evm::aux_structures::{LogQuery, Timestamp};
use zksync_basic_types::AccountTreeId;
use zksync_utils::u256_to_h256;

use crate::{StorageKey, StorageValue, U256};

// TODO (SMA-1269): Refactor StorageLog/StorageLogQuery and StorageLogKind/StorageLongQueryType.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StorageLogKind {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StorageLog {
    pub kind: StorageLogKind,
    pub key: StorageKey,
    pub value: StorageValue,
}

impl StorageLog {
    pub fn from_log_query(log: &StorageLogQuery) -> Self {
        let key = StorageKey::new(
            AccountTreeId::new(log.log_query.address),
            u256_to_h256(log.log_query.key),
        );
        if log.log_query.rw_flag {
            if log.log_query.rollback {
                Self::new_write_log(key, u256_to_h256(log.log_query.read_value))
            } else {
                Self::new_write_log(key, u256_to_h256(log.log_query.written_value))
            }
        } else {
            Self::new_read_log(key, u256_to_h256(log.log_query.read_value))
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
            kind: StorageLogKind::Write,
            key,
            value,
        }
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
            rw_flag: matches!(self.kind, StorageLogKind::Write),
            rollback: false,
            is_service: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum StorageLogQueryType {
    Read,
    InitialWrite,
    RepeatedWrite,
}

/// Log query, which handle initial and repeated writes to the storage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageLogQuery {
    pub log_query: LogQuery,
    pub log_type: StorageLogQueryType,
}

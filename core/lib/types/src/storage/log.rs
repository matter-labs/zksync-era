use crate::{H160, U256};
use serde::{Deserialize, Serialize};
use zk_evm::aux_structures::{LogQuery, Timestamp};
use zkevm_test_harness::witness::sort_storage_access::LogQueryLike;
use zksync_basic_types::AccountTreeId;
use zksync_utils::u256_to_h256;

use super::{StorageKey, StorageValue, H256};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StorageLogKind {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    /// Encodes the log key and value into a byte sequence.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Concatenate account, key and value.
        let mut output = self.key.account().to_fixed_bytes().to_vec();
        output.extend_from_slice(&self.key.key().to_fixed_bytes());
        output.extend_from_slice(self.value.as_fixed_bytes());

        output
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WitnessStorageLog {
    pub storage_log: StorageLog,
    pub previous_value: H256,
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

impl LogQueryLike for StorageLogQuery {
    fn shard_id(&self) -> u8 {
        self.log_query.shard_id
    }

    fn address(&self) -> H160 {
        self.log_query.address
    }

    fn key(&self) -> U256 {
        self.log_query.key
    }

    fn rw_flag(&self) -> bool {
        self.log_query.rw_flag
    }

    fn rollback(&self) -> bool {
        self.log_query.rollback
    }

    fn read_value(&self) -> U256 {
        self.log_query.read_value
    }

    fn written_value(&self) -> U256 {
        self.log_query.written_value
    }

    fn create_partially_filled_from_fields(
        shard_id: u8,
        address: H160,
        key: U256,
        read_value: U256,
        written_value: U256,
        rw_flag: bool,
    ) -> Self {
        Self {
            log_type: StorageLogQueryType::Read,
            log_query: LogQuery {
                timestamp: Timestamp::empty(),
                tx_number_in_block: 0,
                aux_byte: 0,
                shard_id,
                address,
                key,
                read_value,
                written_value,
                rw_flag,
                rollback: false,
                is_service: false,
            },
        }
    }
}

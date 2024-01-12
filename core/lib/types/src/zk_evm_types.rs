use zk_evm::{
    aux_structures::Timestamp as Timestamp_1_4_0,
    zk_evm_abstractions::queries::LogQuery as LogQuery_1_4_0,
};
use zk_evm_1_4_1::{
    aux_structures::Timestamp as Timestamp_1_4_1,
    zk_evm_abstractions::queries::LogQuery as LogQuery_1_4_1,
};
use zksync_basic_types::{Address, U256};

// #[derive(Clone, Copy)]
// pub struct EventMessage {
//     pub shard_id: u8,
//     pub is_first: bool,
//     pub tx_number_in_block: u16,
//     pub address: Address,
//     pub key: U256,
//     pub value: U256,
// }

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum FarCallOpcode {
    Normal = 0,
    Delegate,
    Mimic,
}

/// Struct representing the VM timestamp
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, PartialOrd, Ord,
)]
pub struct Timestamp(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct LogQuery {
    pub timestamp: Timestamp,
    pub tx_number_in_block: u16,
    pub aux_byte: u8,
    pub shard_id: u8,
    pub address: Address,
    pub key: U256,
    pub read_value: U256,
    pub written_value: U256,
    pub rw_flag: bool,
    pub rollback: bool,
    pub is_service: bool,
}

impl From<LogQuery> for LogQuery_1_4_0 {
    fn from(log_query: LogQuery) -> Self {
        Self {
            timestamp: Timestamp_1_4_0(log_query.timestamp.0),
            tx_number_in_block: log_query.tx_number_in_block,
            aux_byte: log_query.aux_byte,
            shard_id: log_query.shard_id,
            address: log_query.address,
            key: log_query.key,
            read_value: log_query.read_value,
            written_value: log_query.written_value,
            rw_flag: log_query.rw_flag,
            rollback: log_query.rollback,
            is_service: log_query.is_service,
        }
    }
}

impl From<LogQuery_1_4_0> for LogQuery {
    fn from(log_query: LogQuery_1_4_0) -> Self {
        Self {
            timestamp: Timestamp(log_query.timestamp.0),
            tx_number_in_block: log_query.tx_number_in_block,
            aux_byte: log_query.aux_byte,
            shard_id: log_query.shard_id,
            address: log_query.address,
            key: log_query.key,
            read_value: log_query.read_value,
            written_value: log_query.written_value,
            rw_flag: log_query.rw_flag,
            rollback: log_query.rollback,
            is_service: log_query.is_service,
        }
    }
}

impl From<LogQuery> for LogQuery_1_4_1 {
    fn from(log_query: LogQuery) -> Self {
        Self {
            timestamp: Timestamp_1_4_1(log_query.timestamp.0),
            tx_number_in_block: log_query.tx_number_in_block,
            aux_byte: log_query.aux_byte,
            shard_id: log_query.shard_id,
            address: log_query.address,
            key: log_query.key,
            read_value: log_query.read_value,
            written_value: log_query.written_value,
            rw_flag: log_query.rw_flag,
            rollback: log_query.rollback,
            is_service: log_query.is_service,
        }
    }
}

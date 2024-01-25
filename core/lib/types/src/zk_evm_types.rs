use zksync_basic_types::{Address, U256};

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

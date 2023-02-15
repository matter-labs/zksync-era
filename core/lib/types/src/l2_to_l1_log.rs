use crate::commitment::CommitmentSerializable;
use crate::{Address, H256};
use serde::{Deserialize, Serialize};
use zk_evm::reference_impls::event_sink::EventMessage;
use zksync_utils::u256_to_h256;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, Eq)]
pub struct L2ToL1Log {
    pub shard_id: u8,
    pub is_service: bool,
    pub tx_number_in_block: u16,
    pub sender: Address,
    pub key: H256,
    pub value: H256,
}

impl From<Vec<u8>> for L2ToL1Log {
    fn from(data: Vec<u8>) -> Self {
        assert_eq!(data.len(), L2ToL1Log::SERIALIZED_SIZE);
        Self {
            shard_id: data[0],
            is_service: data[1] != 0,
            tx_number_in_block: u16::from_be_bytes([data[2], data[3]]),
            sender: Address::from_slice(&data[4..24]),
            key: H256::from_slice(&data[24..56]),
            value: H256::from_slice(&data[56..88]),
        }
    }
}

impl From<EventMessage> for L2ToL1Log {
    fn from(m: EventMessage) -> Self {
        Self {
            shard_id: m.shard_id,
            is_service: m.is_first,
            tx_number_in_block: m.tx_number_in_block,
            sender: m.address,
            key: u256_to_h256(m.key),
            value: u256_to_h256(m.value),
        }
    }
}

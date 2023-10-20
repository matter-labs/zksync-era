use crate::commitment::SerializeCommitment;
use crate::{Address, H256};
use serde::{Deserialize, Serialize};
use zk_evm::reference_impls::event_sink::EventMessage;
use zk_evm_1_4_0::reference_impls::event_sink::EventMessage as EventMessage_1_4_0;
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

impl L2ToL1Log {
    /// Legacy upper bound of L2-to-L1 logs per single L1 batch. This is not used as a limit now,
    /// but still determines the minimum number of items in the Merkle tree built from L2-to-L1 logs
    /// for a certain batch.
    pub const LEGACY_LIMIT_PER_L1_BATCH: usize = 2048;

    pub fn from_slice(data: &[u8]) -> Self {
        assert_eq!(data.len(), Self::SERIALIZED_SIZE);
        Self {
            shard_id: data[0],
            is_service: data[1] != 0,
            tx_number_in_block: u16::from_be_bytes([data[2], data[3]]),
            sender: Address::from_slice(&data[4..24]),
            key: H256::from_slice(&data[24..56]),
            value: H256::from_slice(&data[56..88]),
        }
    }

    /// Converts this log to a byte array by serializing it as a commitment.
    pub fn to_bytes(&self) -> [u8; Self::SERIALIZED_SIZE] {
        let mut buffer = [0_u8; Self::SERIALIZED_SIZE];
        self.serialize_commitment(&mut buffer);
        buffer
    }

    pub fn packed_encoding(&self) -> Vec<u8> {
        let mut res = vec![];
        res.extend_from_slice(&self.shard_id.to_be_bytes());
        res.extend_from_slice(&(self.is_service as u8).to_be_bytes());
        res.extend_from_slice(&self.tx_number_in_block.to_be_bytes());
        res.extend_from_slice(self.sender.as_bytes());
        res.extend(self.key.as_bytes());
        res.extend(self.value.as_bytes());
        res
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

impl From<EventMessage_1_4_0> for L2ToL1Log {
    fn from(m: EventMessage_1_4_0) -> Self {
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

use serde::{Deserialize, Serialize};

use std::cmp::Ordering;

use crate::{
    l1::{OpProcessingType, PriorityQueueType},
    H256, U256,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriorityOpOnchainData {
    pub layer_2_tip_fee: U256,
    pub onchain_data_hash: H256,
}

impl From<PriorityOpOnchainData> for Vec<u8> {
    fn from(data: PriorityOpOnchainData) -> Vec<u8> {
        let mut raw_data = vec![0u8; 64];
        data.layer_2_tip_fee.to_big_endian(&mut raw_data[..32]);
        raw_data[32..].copy_from_slice(data.onchain_data_hash.as_bytes());
        raw_data
    }
}

impl From<Vec<u8>> for PriorityOpOnchainData {
    fn from(data: Vec<u8>) -> Self {
        Self {
            layer_2_tip_fee: U256::from_big_endian(&data[..32]),
            onchain_data_hash: H256::from_slice(&data[32..]),
        }
    }
}

impl Eq for PriorityOpOnchainData {}

impl PartialOrd for PriorityOpOnchainData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityOpOnchainData {
    fn cmp(&self, other: &Self) -> Ordering {
        self.layer_2_tip_fee.cmp(&other.layer_2_tip_fee)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriorityOpOnchainMetadata {
    pub op_processing_type: OpProcessingType,
    pub priority_queue_type: PriorityQueueType,
    pub onchain_data: PriorityOpOnchainData,
}

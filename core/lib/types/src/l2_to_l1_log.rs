use serde::{Deserialize, Serialize};
use zksync_system_constants::{BLOB1_LINEAR_HASH_KEY_PRE_GATEWAY, PUBDATA_CHUNK_PUBLISHER_ADDRESS};

use crate::{
    blob::{num_blobs_created, num_blobs_required},
    commitment::SerializeCommitment,
    Address, ProtocolVersionId, H256,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, Eq)]
pub struct L2ToL1Log {
    pub shard_id: u8,
    pub is_service: bool,
    pub tx_number_in_block: u16,
    pub sender: Address,
    pub key: H256,
    pub value: H256,
}

/// A struct representing a "user" L2->L1 log, i.e. the one that has been emitted by using the L1Messenger.
/// It is identical to the SystemL2ToL1Log struct, but
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, Eq)]
pub struct UserL2ToL1Log(pub L2ToL1Log);

/// A struct representing a "user" L2->L1 log, i.e. the one that has been emitted by using the L1Messenger.
/// It is identical to the SystemL2ToL1Log struct, but

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, Eq)]
pub struct SystemL2ToL1Log(pub L2ToL1Log);

impl L2ToL1Log {
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

/// Returns the number of items in the Merkle tree built from L2-to-L1 logs
/// for a certain protocol version.
pub fn l2_to_l1_logs_tree_size(protocol_version: ProtocolVersionId) -> usize {
    pub const PRE_BOOJUM_L2_L1_LOGS_TREE_SIZE: usize = 512;
    pub const VM_1_4_0_L2_L1_LOGS_TREE_SIZE: usize = 2048;
    pub const VM_1_4_2_L2_L1_LOGS_TREE_SIZE: usize = 4096;
    pub const VM_1_5_0_L2_L1_LOGS_TREE_SIZE: usize = 16384;

    if protocol_version.is_pre_boojum() {
        PRE_BOOJUM_L2_L1_LOGS_TREE_SIZE
    } else if protocol_version.is_1_4_0() || protocol_version.is_1_4_1() {
        VM_1_4_0_L2_L1_LOGS_TREE_SIZE
    } else if protocol_version.is_pre_1_5_0() {
        VM_1_4_2_L2_L1_LOGS_TREE_SIZE
    } else {
        VM_1_5_0_L2_L1_LOGS_TREE_SIZE
    }
}

/// Returns the blob hashes parsed out from the system logs
pub fn parse_system_logs_for_blob_hashes_pre_gateway(
    protocol_version: &ProtocolVersionId,
    system_logs: &[SystemL2ToL1Log],
) -> Vec<H256> {
    assert!(
        protocol_version.is_pre_gateway(),
        "Cannot parse blob linear hashes from system logs for post gateway"
    );

    let num_required_blobs = num_blobs_required(protocol_version) as u32;
    let num_created_blobs = num_blobs_created(protocol_version) as u32;

    if num_created_blobs == 0 {
        return vec![H256::zero(); num_required_blobs as usize];
    }

    let mut blob_hashes = system_logs
        .iter()
        .filter(|log| {
            log.0.sender == PUBDATA_CHUNK_PUBLISHER_ADDRESS
                && log.0.key >= H256::from_low_u64_be(BLOB1_LINEAR_HASH_KEY_PRE_GATEWAY as u64)
                && log.0.key
                    < H256::from_low_u64_be(
                        (BLOB1_LINEAR_HASH_KEY_PRE_GATEWAY + num_created_blobs) as u64,
                    )
        })
        .map(|log| (log.0.key, log.0.value))
        .collect::<Vec<(H256, H256)>>();

    blob_hashes.sort_unstable_by_key(|(k, _)| *k);
    let mut blob_hashes = blob_hashes.iter().map(|(_, v)| *v).collect::<Vec<H256>>();
    blob_hashes.resize(num_required_blobs as usize, H256::zero());
    blob_hashes
}

#[cfg(test)]
mod tests {
    use zksync_basic_types::U256;
    use zksync_system_constants::L1_MESSENGER_ADDRESS;
    use zksync_utils::u256_to_h256;

    use super::L2ToL1Log;

    #[test]
    fn l2_to_l1_log_to_bytes() {
        let expected_log_bytes = [
            0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 8, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 19,
        ];

        let log = L2ToL1Log {
            shard_id: 0u8,
            is_service: false,
            tx_number_in_block: 6u16,
            sender: L1_MESSENGER_ADDRESS,
            key: u256_to_h256(U256::from(11)),
            value: u256_to_h256(U256::from(19)),
        };

        assert_eq!(expected_log_bytes, log.to_bytes());
    }
}

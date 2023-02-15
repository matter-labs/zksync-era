use crate::H256;
use serde::{Deserialize, Serialize};
use zksync_basic_types::U256;

/// In vm there are two types of writes Initial and Repeated. After the first write to the leaf we assign an index to it
/// and in the future we should use index instead of full key. It allows us to compress the data.
#[derive(Clone, Debug, Deserialize, Serialize, Default, Eq, PartialEq)]
pub struct InitialStorageWrite {
    pub key: U256,
    pub value: H256,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default, Eq, PartialEq)]
pub struct RepeatedStorageWrite {
    pub index: u64,
    pub value: H256,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commitment::serialize_commitments;
    use crate::{H256, U256};

    #[test]
    fn calculate_hash_for_storage_writes() {
        let initial_writes = vec![
            InitialStorageWrite {
                key: U256::from(1u32),
                value: H256::from([1; 32]),
            },
            InitialStorageWrite {
                key: U256::from(2u32),
                value: H256::from([3; 32]),
            },
        ];
        let bytes = serialize_commitments(&initial_writes);
        let size = "00000002";
        let initial_write_1= "01000000000000000000000000000000000000000000000000000000000000000101010101010101010101010101010101010101010101010101010101010101";
        let initial_write_2= "02000000000000000000000000000000000000000000000000000000000000000303030303030303030303030303030303030303030303030303030303030303";
        let expected_bytes =
            hex::decode(format!("{}{}{}", size, initial_write_1, initial_write_2)).unwrap();
        assert_eq!(expected_bytes, bytes);
        let repeated_writes = vec![
            RepeatedStorageWrite {
                index: 1,
                value: H256::from([1; 32]),
            },
            RepeatedStorageWrite {
                index: 2,
                value: H256::from([3; 32]),
            },
        ];
        let bytes = serialize_commitments(&repeated_writes);
        let size = "00000002";
        let repeated_write_1 =
            "00000000000000010101010101010101010101010101010101010101010101010101010101010101";
        let repeated_write_2 =
            "00000000000000020303030303030303030303030303030303030303030303030303030303030303";
        let expected_bytes =
            hex::decode(format!("{}{}{}", size, repeated_write_1, repeated_write_2)).unwrap();
        assert_eq!(expected_bytes, bytes);
    }
}

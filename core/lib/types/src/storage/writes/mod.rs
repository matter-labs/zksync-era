use std::{convert::TryInto, fmt};

use serde::{de, ser::SerializeTuple, Deserialize, Deserializer, Serialize, Serializer};
use zksync_basic_types::{Address, U256};

pub(crate) use self::compression::{compress_with_best_strategy, COMPRESSION_VERSION_NUMBER};
use crate::H256;

pub mod compression;

/// The number of bytes being used for state diff enumeration indices. Applicable to repeated writes.
pub const BYTES_PER_ENUMERATION_INDEX: u8 = 4;
/// The number of bytes being used for state diff derived keys. Applicable to initial writes.
pub const BYTES_PER_DERIVED_KEY: u8 = 32;

/// Total byte size of all fields in StateDiffRecord struct
/// 20 + 32 + 32 + 8 + 32 + 32
const STATE_DIFF_RECORD_SIZE: usize = 156;

// 2 * 136 - the size that allows for two keccak rounds.
pub const PADDED_ENCODED_STORAGE_DIFF_LEN_BYTES: usize = 272;

/// In vm there are two types of writes Initial and Repeated. After the first write to the key,
/// we assign an index to it and in the future we should use index instead of full key.
/// It allows us to compress the data, as the full key would use 32 bytes, and the index can be
/// represented only as BYTES_PER_ENUMERATION_INDEX bytes
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
pub struct InitialStorageWrite {
    pub index: u64,
    pub key: U256,
    pub value: H256,
}

/// For repeated writes, we can substitute the 32 byte key for a BYTES_PER_ENUMERATION_INDEX byte index
/// representing its leaf index in the tree.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
pub struct RepeatedStorageWrite {
    pub index: u64,
    pub value: H256,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default, Eq, PartialEq)]
pub struct StateDiffRecord {
    /// address state diff occurred at
    pub address: Address,
    /// storage slot key updated
    pub key: U256,
    /// derived_key == Blake2s(bytes32(address), key)
    pub derived_key: [u8; 32],
    /// index in tree of state diff
    pub enumeration_index: u64,
    /// previous value
    pub initial_value: U256,
    /// updated value
    pub final_value: U256,
}

impl StateDiffRecord {
    // Serialize into byte representation.
    fn encode(&self) -> [u8; STATE_DIFF_RECORD_SIZE] {
        let mut encoding = [0u8; STATE_DIFF_RECORD_SIZE];
        let mut offset = 0;
        let mut end = 0;

        end += 20;
        encoding[offset..end].copy_from_slice(self.address.as_fixed_bytes());
        offset = end;

        end += 32;
        self.key.to_big_endian(&mut encoding[offset..end]);
        offset = end;

        end += 32;
        encoding[offset..end].copy_from_slice(&self.derived_key);
        offset = end;

        end += 8;
        encoding[offset..end].copy_from_slice(&self.enumeration_index.to_be_bytes());
        offset = end;

        end += 32;
        self.initial_value.to_big_endian(&mut encoding[offset..end]);
        offset = end;

        end += 32;
        self.final_value.to_big_endian(&mut encoding[offset..end]);
        offset = end;

        debug_assert_eq!(offset, encoding.len());

        encoding
    }

    pub fn encode_padded(&self) -> [u8; PADDED_ENCODED_STORAGE_DIFF_LEN_BYTES] {
        let mut extended_state_diff_encoding = [0u8; PADDED_ENCODED_STORAGE_DIFF_LEN_BYTES];
        let packed_encoding = self.encode();
        extended_state_diff_encoding[0..packed_encoding.len()].copy_from_slice(&packed_encoding);

        extended_state_diff_encoding
    }

    /// Decode bytes into StateDiffRecord
    pub fn try_from_slice(data: &[u8]) -> Option<Self> {
        if data.len() == 156 {
            Some(Self {
                address: Address::from_slice(&data[0..20]),
                key: U256::from(&data[20..52]),
                derived_key: data[52..84].try_into().unwrap(),
                enumeration_index: u64::from_be_bytes(data[84..92].try_into().unwrap()),
                initial_value: U256::from(&data[92..124]),
                final_value: U256::from(&data[124..156]),
            })
        } else {
            None
        }
    }

    /// compression follows the following algorithm:
    /// 1. if repeated write:
    ///      entry <- enumeration_index || compressed value
    /// 2. if initial write:
    ///      entry <- blake2(bytes32(address), key) || compressed value
    /// size:
    ///      initial:  max of 65 bytes
    ///      repeated: max of 38 bytes
    ///      before:  156 bytes for each
    pub fn compress(&self) -> Vec<u8> {
        let mut comp_state_diff = match self.enumeration_index {
            0 => self.derived_key.to_vec(),
            enumeration_index if enumeration_index <= u32::MAX.into() => {
                (self.enumeration_index as u32).to_be_bytes().to_vec()
            }
            enumeration_index => panic!("enumeration_index is too large: {}", enumeration_index),
        };

        comp_state_diff.extend(compress_with_best_strategy(
            self.initial_value,
            self.final_value,
        ));

        comp_state_diff
    }

    pub fn is_write_initial(&self) -> bool {
        self.enumeration_index == 0
    }
}

/// Compresses a vector of state diff records according to the following:
/// num_initial writes (u32) || compressed initial writes || compressed repeated writes
pub fn compress_state_diffs(mut state_diffs: Vec<StateDiffRecord>) -> Vec<u8> {
    let mut res = vec![];

    // IMPORTANT: Sorting here is determined by the order expected in the circuits.
    state_diffs.sort_by_key(|rec| (rec.address, rec.key));

    let (initial_writes, repeated_writes): (Vec<_>, Vec<_>) = state_diffs
        .iter()
        .partition(|rec| rec.enumeration_index == 0);

    res.extend((initial_writes.len() as u16).to_be_bytes());
    for state_diff in initial_writes {
        res.extend(state_diff.compress());
    }

    for state_diff in repeated_writes {
        res.extend(state_diff.compress());
    }

    prepend_header(res)
}

/// Adds the header to the beginning of the compressed state diffs so it can be used as part of the overall
/// pubdata. Need to prepend: compression version || number of compressed state diffs || number of bytes used for
/// enumeration index.
fn prepend_header(compressed_state_diffs: Vec<u8>) -> Vec<u8> {
    let mut res = vec![0u8; 5];
    res[0] = COMPRESSION_VERSION_NUMBER;

    res[1..4].copy_from_slice(&(compressed_state_diffs.len() as u32).to_be_bytes()[1..4]);

    res[4] = BYTES_PER_ENUMERATION_INDEX;

    res.extend(compressed_state_diffs);

    res.to_vec()
}

/// Struct for storing tree writes in DB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TreeWrite {
    /// `address` part of storage key.
    pub address: Address,
    /// `key` part of storage key.
    pub key: H256,
    /// Value written.
    pub value: H256,
    /// Leaf index of the slot.
    pub leaf_index: u64,
}

impl Serialize for TreeWrite {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut tup = serializer.serialize_tuple(4)?;
        tup.serialize_element(&self.address.0)?;
        tup.serialize_element(&self.key.0)?;
        tup.serialize_element(&self.value.0)?;
        tup.serialize_element(&self.leaf_index)?;
        tup.end()
    }
}

impl<'de> Deserialize<'de> for TreeWrite {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TreeWriteVisitor;

        impl<'de> de::Visitor<'de> for TreeWriteVisitor {
            type Value = TreeWrite;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a tuple of 4 elements")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<TreeWrite, V::Error>
            where
                V: de::SeqAccess<'de>,
            {
                let address: [u8; 20] = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let key: [u8; 32] = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let value: [u8; 32] = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let leaf_index = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?;

                Ok(TreeWrite {
                    address: Address::from_slice(&address),
                    key: H256::from_slice(&key),
                    value: H256::from_slice(&value),
                    leaf_index,
                })
            }
        }

        deserializer.deserialize_tuple(4, TreeWriteVisitor)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ops::{Add, Sub},
        str::FromStr,
    };

    use super::*;
    use crate::{commitment::serialize_commitments, H256, U256};

    #[test]
    fn calculate_hash_for_storage_writes() {
        let initial_writes = vec![
            InitialStorageWrite {
                index: 1,
                key: U256::from(1u32),
                value: H256::from([1; 32]),
            },
            InitialStorageWrite {
                index: 2,
                key: U256::from(2u32),
                value: H256::from([3; 32]),
            },
        ];
        let bytes = serialize_commitments(&initial_writes);

        let expected_bytes = "0100000000000000000000000000000000000000000000000000000000000000\
            0101010101010101010101010101010101010101010101010101010101010101\
            0200000000000000000000000000000000000000000000000000000000000000\
            0303030303030303030303030303030303030303030303030303030303030303";
        let expected_bytes = hex::decode(expected_bytes).unwrap();
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

        let expected_bytes = "0000000000000001\
            0101010101010101010101010101010101010101010101010101010101010101\
            0000000000000002\
            0303030303030303030303030303030303030303030303030303030303030303";
        let expected_bytes = hex::decode(expected_bytes).unwrap();
        assert_eq!(expected_bytes, bytes);
    }

    #[test]
    fn test_compression() {
        let initial_add = StateDiffRecord {
            address: Address::from_str("0x09610c49cfe4a0509dbe319886eb0cfc01f2cfd1").unwrap(),
            key: U256::from(1u8),
            derived_key: [1u8; 32],
            enumeration_index: 0u64,
            initial_value: U256::default(),
            final_value: U256::from(64u8),
        };

        let initial_sub = StateDiffRecord {
            address: Address::from_str("0x1c915d9b098ecf548d978ef9931a09e4e2167fab").unwrap(),
            key: U256::from(2u8),
            derived_key: [2u8; 32],
            enumeration_index: 0u64,
            initial_value: U256::from(64u8),
            final_value: U256::from(20u8),
        };

        let initial_transform = StateDiffRecord {
            address: Address::from_str("0x3859d669dcc980c3ba68806a8d49bbc998da781d").unwrap(),
            key: U256::from(3u8),
            derived_key: [3u8; 32],
            enumeration_index: 0u64,
            initial_value: U256::MAX,
            final_value: U256::from(255u8),
        };

        let initial_none = StateDiffRecord {
            address: Address::from_str("0x3f441bf60f4d8f7704b262f41b7b015c21623f46").unwrap(),
            key: U256::from(5u8),
            derived_key: [4u8; 32],
            enumeration_index: 0u64,
            initial_value: U256::MAX / 2,
            final_value: U256::MAX,
        };

        let repeated_add = StateDiffRecord {
            address: Address::from_str("0x5e52dd6d60c2f89b5ced2bddf53794f0d8c58254").unwrap(),
            key: U256::from(1u8),
            derived_key: [5u8; 32],
            enumeration_index: 1u64,
            initial_value: U256::default(),
            final_value: U256::from(64u8),
        };

        let repeated_sub = StateDiffRecord {
            address: Address::from_str("0x9bac6b5cb15aa5f80f9480af6b530ecd93a30a41").unwrap(),
            key: U256::from(2u8),
            derived_key: [6u8; 32],
            enumeration_index: 2u64,
            initial_value: U256::from(64u8),
            final_value: U256::from(20u8),
        };

        let repeated_transform = StateDiffRecord {
            address: Address::from_str("0xaf21caa263eefa213301522c1062d22a890b2b6d").unwrap(),
            key: U256::from(3u8),
            derived_key: [7u8; 32],
            enumeration_index: 3u64,
            initial_value: U256::MAX,
            final_value: U256::from(255u8),
        };

        let repeated_none = StateDiffRecord {
            address: Address::from_str("0xb21058b7c589c49871a295575418e9e3edaf44b0").unwrap(),
            key: U256::from(5u8),
            derived_key: [8u8; 32],
            enumeration_index: 5u64,
            initial_value: U256::MAX / 2,
            final_value: U256::MAX,
        };

        let storage_diffs = vec![
            initial_add,
            initial_sub,
            initial_transform,
            initial_none,
            repeated_add,
            repeated_sub,
            repeated_transform,
            repeated_none,
        ];

        let compressed_state_diffs = compress_state_diffs(storage_diffs.clone());

        let mut storage_diffs = storage_diffs.clone();
        storage_diffs.sort_by_key(|rec| (rec.address, rec.key));

        let (header, compressed_state_diffs) = compressed_state_diffs.split_at(5);

        assert!(header[0] == COMPRESSION_VERSION_NUMBER);
        assert!(U256::from(&header[1..4]) == U256::from(compressed_state_diffs.len()));
        assert!(header[4] == 4u8);

        let (num_initial, compressed_state_diffs) = compressed_state_diffs.split_at(2);
        assert!(num_initial[0] == 0u8);
        assert!(num_initial[1] == 4u8);

        let (initial, repeated): (Vec<_>, Vec<_>) =
            storage_diffs.iter().partition(|v| v.enumeration_index == 0);
        assert!((initial.len() as u8) == num_initial[1]);

        // Initial
        let (key, compressed_state_diffs) = compressed_state_diffs.split_at(32);
        assert!(U256::from(key) == U256::from(initial[0].derived_key));
        let (metadata, compressed_state_diffs) = compressed_state_diffs.split_at(1);
        let metadata = metadata[0];
        let operation = metadata & 7;
        let len = (metadata >> 3) as usize;
        verify_value(
            initial[0].initial_value,
            initial[0].final_value,
            operation,
            &compressed_state_diffs.to_vec()[..len],
        );
        let compressed_state_diffs = &compressed_state_diffs[len..];

        let (key, compressed_state_diffs) = compressed_state_diffs.split_at(32);
        assert!(U256::from(key) == U256::from(initial[1].derived_key));
        let (metadata, compressed_state_diffs) = compressed_state_diffs.split_at(1);
        let metadata = metadata[0];
        let operation = metadata & 7;
        let len = (metadata >> 3) as usize;
        verify_value(
            initial[1].initial_value,
            initial[1].final_value,
            operation,
            &compressed_state_diffs.to_vec()[..len],
        );
        let compressed_state_diffs = &compressed_state_diffs[len..];

        let (key, compressed_state_diffs) = compressed_state_diffs.split_at(32);
        assert!(U256::from(key) == U256::from(initial[2].derived_key));
        let (metadata, compressed_state_diffs) = compressed_state_diffs.split_at(1);
        let metadata = metadata[0];
        let operation = metadata & 7;
        let len = (metadata >> 3) as usize;
        verify_value(
            initial[2].initial_value,
            initial[2].final_value,
            operation,
            &compressed_state_diffs.to_vec()[..len],
        );
        let compressed_state_diffs = &compressed_state_diffs[len..];

        let (key, compressed_state_diffs) = compressed_state_diffs.split_at(32);
        assert!(U256::from(key) == U256::from(initial[3].derived_key));
        let (metadata, compressed_state_diffs) = compressed_state_diffs.split_at(1);
        let metadata = metadata[0];
        let operation = metadata & 7;
        verify_value(
            initial[3].initial_value,
            initial[3].final_value,
            operation,
            &compressed_state_diffs.to_vec()[..32],
        );
        let compressed_state_diffs = &compressed_state_diffs[32..];

        // Repeated
        let (enum_index, compressed_state_diffs) = compressed_state_diffs.split_at(4);
        assert!((enum_index[3] as u64) == repeated[0].enumeration_index);
        let (metadata, compressed_state_diffs) = compressed_state_diffs.split_at(1);
        let metadata = metadata[0];
        let operation = metadata & 7;
        let len = (metadata >> 3) as usize;
        verify_value(
            repeated[0].initial_value,
            repeated[0].final_value,
            operation,
            &compressed_state_diffs.to_vec()[..len],
        );
        let compressed_state_diffs = &compressed_state_diffs[len..];

        let (enum_index, compressed_state_diffs) = compressed_state_diffs.split_at(4);
        assert!((enum_index[3] as u64) == repeated[1].enumeration_index);
        let (metadata, compressed_state_diffs) = compressed_state_diffs.split_at(1);
        let metadata = metadata[0];
        let operation = metadata & 7;
        let len = (metadata >> 3) as usize;
        verify_value(
            repeated[1].initial_value,
            repeated[1].final_value,
            operation,
            &compressed_state_diffs.to_vec()[..len],
        );
        let compressed_state_diffs = &compressed_state_diffs[len..];

        let (enum_index, compressed_state_diffs) = compressed_state_diffs.split_at(4);
        assert!((enum_index[3] as u64) == repeated[2].enumeration_index);
        let (metadata, compressed_state_diffs) = compressed_state_diffs.split_at(1);
        let metadata = metadata[0];
        let operation = metadata & 7;
        let len = (metadata >> 3) as usize;
        verify_value(
            repeated[2].initial_value,
            repeated[2].final_value,
            operation,
            &compressed_state_diffs.to_vec()[..len],
        );
        let compressed_state_diffs = &compressed_state_diffs[len..];

        let (enum_index, compressed_state_diffs) = compressed_state_diffs.split_at(4);
        assert!((enum_index[3] as u64) == repeated[3].enumeration_index);
        let (metadata, compressed_state_diffs) = compressed_state_diffs.split_at(1);
        let metadata = metadata[0];
        let operation = metadata & 7;
        verify_value(
            repeated[3].initial_value,
            repeated[3].final_value,
            operation,
            &compressed_state_diffs.to_vec()[..32],
        );
        let compressed_state_diffs = &compressed_state_diffs[32..];

        assert!(compressed_state_diffs.is_empty());
    }

    #[test]
    fn test_encoding() {
        let state_diff = StateDiffRecord {
            address: Address::from_str("0x09610c49cfe4a0509dbe319886eb0cfc01f2cfd1").unwrap(),
            key: U256::from(1u8),
            derived_key: [1u8; 32],
            enumeration_index: 0u64,
            initial_value: U256::default(),
            final_value: U256::from(64u8),
        };

        let encoded = state_diff.encode();
        let encoded_state_diff = hex::encode(encoded);

        let expected_encoding = "09610c49cfe4a0509dbe319886eb0cfc01f2cfd1000000000000\
            000000000000000000000000000000000000000000000000000101010101010101010101010101010\
            101010101010101010101010101010101010000000000000000000000000000000000000000000000\
            000000000000000000000000000000000000000000000000000000000000000000000000000000000\
            00000000000000040"
            .to_string();

        assert_eq!(encoded_state_diff, expected_encoding);

        let encode_padded = state_diff.encode_padded();
        let encoded_padded_state_diff = hex::encode(encode_padded);

        let expected_padded_encoding = "09610c49cfe4a0509dbe319886eb0cfc01f2cfd100000\
            000000000000000000000000000000000000000000000000000000000010101010101010101010101\
            010101010101010101010101010101010101010101000000000000000000000000000000000000000\
            000000000000000000000000000000000000000000000000000000000000000000000000000000000\
            000000000000000000000040000000000000000000000000000000000000000000000000000000000\
            000000000000000000000000000000000000000000000000000000000000000000000000000000000\
            000000000000000000000000000000000000000000000000000000000000000000000000000000000\
            0000000000000"
            .to_string();

        assert_eq!(encoded_padded_state_diff, expected_padded_encoding);
    }

    fn verify_value(
        initial_value: U256,
        final_value: U256,
        operation: u8,
        compressed_value: &[u8],
    ) {
        if operation == 0 || operation == 3 {
            assert!(U256::from(compressed_value) == final_value);
        } else if operation == 1 {
            assert!(initial_value.add(U256::from(compressed_value)) == final_value);
        } else if operation == 2 {
            assert!(initial_value.sub(U256::from(compressed_value)) == final_value);
        } else {
            panic!("invalid operation id");
        }
    }

    #[test]
    fn check_tree_write_serde() {
        let tree_write = TreeWrite {
            address: Address::repeat_byte(0x11),
            key: H256::repeat_byte(0x22),
            value: H256::repeat_byte(0x33),
            leaf_index: 1,
        };

        let serialized = bincode::serialize(&tree_write).unwrap();
        let expected: Vec<_> = vec![0x11u8; 20]
            .into_iter()
            .chain(vec![0x22u8; 32])
            .chain(vec![0x33u8; 32])
            .chain(1u64.to_le_bytes())
            .collect();
        assert_eq!(serialized, expected);

        let deserialized: TreeWrite = bincode::deserialize(&serialized).unwrap();
        assert_eq!(tree_write, deserialized);
    }
}

/*



000000070001000000000000000000000000000000000000000080010b198075f23eba8137d7c071e5b9e594a4acabb85dfbd59b4b5dd326a54671ed000000000000000000000000000000000000000000000000000000000000000100010001000000000000000000000000000000000000800191a487c94f7f5752ba5b22e66ec908a18459fdab5afd7f784d3ebec26825297900000000000000000000000000000000000000000000000000000000000000010001000200000000000000000000000000000000000080012e02113ea86c32bad4f949c1fd59c2bdce8bfef509c1099fd4c3ca2e566f6e58000000000000000000000000000000000000000000000000000000000000000100010003000000000000000000000000000000000000800149c5d8c0374361254dcefb02ce84558490e44918871ed28cd357a339cdeefe080000000000000000000000000000000000000000000000000000000000000001000100040000000000000000000000000000000000008001b0906bdf1e67f14a43058f90906fd8187fd61514dd013b3d3a505da1c3c6f8dc0000000000000000000000000000000000000000000000000000000000000001000100050000000000000000000000000000000000008001cbbd9fab11daca9871dca880c0afdf4a642065726c96ea4e459b2e5741e03bb3000000000000000000000000000000000000000000000000000000000000000100010006000000000000000000000000000000000000800147c334b4ca442a98afc204272c1782dd8943b392ff9c75f39eaccaa6d7a904d4000000000000000000000000000000000000000000000000000000000000000100000000000000000100079404002850067c298622a3e4cfa9cde6ecff67aa2da9f870ca7c0ccb2f434dc1df65b16b00010000cbacfe9317411fc97202d4074acbaebf0032eceeffd580e4a8a7e5872913bee89862705fefce5e5cda17344ad5ba112ad3effc8c4d9c6e4f4558c67aa100010000691fa4f751f8312bc555242f18ed78cdc9aabc0ea77d7d5a675ee8ac6fdeeb49b86e07758652db7a606fb2bbc968fbcd75834e26ae17f8ed8ed7d47bb1000100015d49d29767b868317d524a1ef8985652932d4be89964953a01931026c92a225450f25463492d6fe0d547fb2b2d4e4536217887eedc183f05e90045e9b100010001f909cf8e16ddeb08c19965b7d1f362dc33bd94a9fd3fc282daa9f6deb9741e0b610937ab050b5810ca834703b65de3beb6a0b04a9b7562d11e9fed07ac000100047921bf49b783c0687b9c1594366171a3ba63b916e5894a00fa1184bdf9f257c141880a838c0ffbdfd490de5f14c3458313ea387e0c3a2d39c6296756fd00010001915d77524723b4d66b72f62b3390d4875e0845b432d1244000334456167e44a311e3a4f4c0db49b982086e38fec6b9911f5e868aa999953f2b1c363e57000100005b75ef1bed7be47d27d372a69fbeac5eb64a51c47efd6b1d488408c2a864677de683cf9870ddeb1150e7aa22b608322cd20612d05dfd2c4f5b0f9fdc85890200000000000000000000000000000000ab63c4cebbd508a7d7184f0b9134453eea7a09ca749610d5576f8046241b9cde8905000000000000000000000000000000008edd044ac3e812ea7a42a2bdb3e580b6d1e581e5ebbb21368f9570972380e7320901c90cc02810b9c5f7af23337717fd255535f4a0f8b918479cadf2126802cde5280901c904134d8e764daff1a3530b1986b7404c809fff598dc906043a1bfe135f5d550901a161b238d32452c5f2cc28dc43be48ed11a89e47bcaa1ba8879814c16d72b465090135c315a518cbff58da6290dd2f9a2e6598431133b4ea5c3f837cee83a25bcfcd0901f3fb235c63b612eb7c0c5739451af0fdc5f014a19148054a74ba704a3f4645df0901d684c68a9ac5173e43b36e1af8954c64576336d871ae20316f848d7d1432c3500901eba9d3385189120a4f2933cebb8233c028f22dbc71ed5c2daeb93cfe02d7c4cb0901ec0ca8527782cc4883763fe261fe5882cc50c7badf137063223edd516ed11f8b090971e91721f9918576d760f02f03cac47c6f4003316031848e3c1d99e6e83a474341019c7855ba6c40002d5a6962cccee5d4adb48a36bbbf443a531721484381125937f3001ac5ff875b3901fafd77b549307b727eb806c926fb20c8ad087c57422977cebd06373e26d19b640e5fe32c85ff41019a7d5842b6f6d0a25420c1d9d705358c134cc601d9d184cb4dfdde7e1cac2bc3d4d38bf9ec44e62105f5e100123bafc586f77764488cd24c6a77546e5a0fe8bdfb4fa203cfaffc36cce4dd5b89010000000000000000000000006672be378e7dd06ac5b73b473be6bc5a51030f4c7437657cb7b29bf376c564b8d1675a5e89020000000000000000000000006672be384ba84e1f37d041bc6e55ba396826cc494e84d4815b6db52690422eea7386314f00e8e77626586f73b955364c7b4bbf0bb7f7685ebd40e852b164633a4acbd3244c3de2202ccb626ad387d70722e64fbe44562e2f231a290c08532b8d6aba402ff500f8498fc2fc0cc0f5ccf4bbde76945898d11db94461dd1e81b57a93a7039c411b79093588d0e847efa73a10ce20e4799fb1e46642d65617c7e5213fa04989d92d89020000000000000000000000006672be3887ded247e1660f827071c7f1371934589751085384fc9f4462c1f1897c5c3eef89010000000000000000000000000000000186248193eb4dd2a8ce815f876c124d48359522f0854d95d8072eaff0d37d55bd1103203e890d6c2c3bada6eecc9603a99c1c6259ed5a6402f1c76cc18b568c3aefba0f110574911dd2ad743ff237d411648a0fe32c6d74eec060716a2a74352f6b1c435b5d67000ed3937da1db366021236aa52cc275d78bd86e7c2f3aec292dbdbab2869a2609f7d7090ac2180d9e62361e3733e3c5e9488e72bbcc2c2fbb23353642ed7c1f92a16dfa58e9a7a87f0b7c4ecc78b8106b9d89f5458263a0a58d2b26f1c19756452af08ee8e9e194770edd85cc70fae38b769ad84deca1c00ac10698d20b59b924e8468be0a48a6fe48d347d206df061fe09a32f89dd3a1fba7ce9e6baece53dd8e4e147171681b0f6de1cb107ca4dde9ce4f47ce96f8abf48f773b54f3a950800024329c170fbb6a150db52e973833627634d61cf6f67fb5c59a993b145f5d5c81da10a8d3b4054f2e55434c683ddbd7f929e9a9608e6be8c432faf9ced61aa47b4a165bbcb74961be8bed0abca715365ce305af4733100010001211b0c33353cdf7a320f768e3dc40bce1326d639fcac099bba9ecd8e34eaf192d25f4ef91464429e7230120bfa4208903f9126cda586bae576f470666ea19cdd75c874579afee6be7a2ae0287baa91549be3c9dfcfbadbc90f83519ef7fcb108cb0e3139b09b93089c181eec46dd4bad7653a16dfa58e9a7a87f0b7c4ecc78b8106b9d89f545824fb16b1fbf16d9c14f6fba40896c133402901cf21d9906dc69b8ecedec157b3309ff69714774ff9171ea92bccd78d4f78e3d6c5dee127fe88e0e50870b77ed5e50ae09ff00000000000000000000000000000000000000



000000070001000000000000000000000000000000000000000080010b198075f23eba8137d7c071e5b9e594a4acabb85dfbd59b4b5dd326a54671ed000000000000000000000000000000000000000000000000000000000000000100010001000000000000000000000000000000000000800191a487c94f7f5752ba5b22e66ec908a18459fdab5afd7f784d3ebec26825297900000000000000000000000000000000000000000000000000000000000000010001000200000000000000000000000000000000000080012e02113ea86c32bad4f949c1fd59c2bdce8bfef509c1099fd4c3ca2e566f6e58000000000000000000000000000000000000000000000000000000000000000100010003000000000000000000000000000000000000800149c5d8c0374361254dcefb02ce84558490e44918871ed28cd357a339cdeefe080000000000000000000000000000000000000000000000000000000000000001000100040000000000000000000000000000000000008001b0906bdf1e67f14a43058f90906fd8187fd61514dd013b3d3a505da1c3c6f8dc0000000000000000000000000000000000000000000000000000000000000001000100050000000000000000000000000000000000008001cbbd9fab11daca9871dca880c0afdf4a642065726c96ea4e459b2e5741e03bb3000000000000000000000000000000000000000000000000000000000000000100010006000000000000000000000000000000000000800147c334b4ca442a98afc204272c1782dd8943b392ff9c75f39eaccaa6d7a904d4000000000000000000000000000000000000000000000000000000000000000100000000000000000100079404002850067c298622a3e4cfa9cde6ecff67aa2da9f870ca7c0ccb2f434dc1df65b16b00010000cbacfe9317411fc97202d4074acbaebf0032eceeffd580e4a8a7e5872913bee89862705fefce5e5cda17344ad5ba112ad3effc8c4d9c6e4f4558c67aa100010000691fa4f751f8312bc555242f18ed78cdc9aabc0ea77d7d5a675ee8ac6fdeeb49b86e07758652db7a606fb2bbc968fbcd75834e26ae17f8ed8ed7d47bb1000100015d49d29767b868317d524a1ef8985652932d4be89964953a01931026c92a225450f25463492d6fe0d547fb2b2d4e4536217887eedc183f05e90045e9b100010001f909cf8e16ddeb08c19965b7d1f362dc33bd94a9fd3fc282daa9f6deb9741e0b610937ab050b5810ca834703b65de3beb6a0b04a9b7562d11e9fed07ac000100047921bf49b783c0687b9c1594366171a3ba63b916e5894a00fa1184bdf9f257c141880a838c0ffbdfd490de5f14c3458313ea387e0c3a2d39c6296756fd00010001915d77524723b4d66b72f62b3390d4875e0845b432d1244000334456167e44a311e3a4f4c0db49b982086e38fec6b9911f5e868aa999953f2b1c363e57000100005b75ef1bed7be47d27d372a69fbeac5eb64a51c47efd6b1d488408c2a864677de683cf9870ddeb1150e7aa22b608322cd20612d05dfd2c4f5b0f9fdc85890200000000000000000000000000000000ab63c4cebbd508a7d7184f0b9134453eea7a09ca749610d5576f8046241b9cde8905000000000000000000000000000000008edd044ac3e812ea7a42a2bdb3e580b6d1e581e5ebbb21368f9570972380e7320901c90cc02810b9c5f7af23337717fd255535f4a0f8b918479cadf2126802cde5280901c904134d8e764daff1a3530b1986b7404c809fff598dc906043a1bfe135f5d550901a161b238d32452c5f2cc28dc43be48ed11a89e47bcaa1ba8879814c16d72b465090135c315a518cbff58da6290dd2f9a2e6598431133b4ea5c3f837cee83a25bcfcd0901f3fb235c63b612eb7c0c5739451af0fdc5f014a19148054a74ba704a3f4645df0901d684c68a9ac5173e43b36e1af8954c64576336d871ae20316f848d7d1432c3500901eba9d3385189120a4f2933cebb8233c028f22dbc71ed5c2daeb93cfe02d7c4cb0901ec0ca8527782cc4883763fe261fe5882cc50c7badf137063223edd516ed11f8b090971e91721f9918576d760f02f03cac47c6f4003316031848e3c1d99e6e83a474341019c7855ba6c40002d5a6962cccee5d4adb48a36bbbf443a531721484381125937f3001ac5ff875b3901fafd77b549307b727eb806c926fb20c8ad087c57422977cebd06373e26d19b640e5fe32c85ff41019a7d5842b6f6d0a25420c1d9d705358c134cc601d9d184cb4dfdde7e1cac2bc3d4d38bf9ec44e62105f5e100123bafc586f77764488cd24c6a77546e5a0fe8bdfb4fa203cfaffc36cce4dd5b89010000000000000000000000006672be378e7dd06ac5b73b473be6bc5a51030f4c7437657cb7b29bf376c564b8d1675a5e89020000000000000000000000006672be384ba84e1f37d041bc6e55ba396826cc494e84d4815b6db52690422eea7386314f00e8e77626586f73b955364c7b4bbf0bb7f7685ebd40e852b164633a4acbd3244c3de2202ccb626ad387d70722e64fbe44562e2f231a290c08532b8d6aba402ff500f8498fc2fc0cc0f5ccf4bbde76945898d11db94461dd1e81b57a93a7039c411b79093588d0e847efa73a10ce20e4799fb1e46642d65617c7e5213fa04989d92d89020000000000000000000000006672be3887ded247e1660f827071c7f1371934589751085384fc9f4462c1f1897c5c3eef89010000000000000000000000000000000186248193eb4dd2a8ce815f876c124d48359522f0854d95d8072eaff0d37d55bd1103203e890d6c2c3bada6eecc9603a99c1c6259ed5a6402f1c76cc18b568c3aefba0f110574911dd2ad743ff237d411648a0fe32c6d74eec060716a2a74352f6b1c435b5d67000ed3937da1db366021236aa52cc275d78bd86e7c2f3aec292dbdbab2869a2609f7d7090ac2180d9e62361e3733e3c5e9488e72bbcc2c2fbb23353642ed7c1f92a16dfa58e9a7a87f0b7c4ecc78b8106b9d89f5458263a0a58d2b26f1c19756452af08ee8e9e194770edd85cc70fae38b769ad84deca1c00ac10698d20b59b924e8468be0a48a6fe48d347d206df061fe09a32f89dd3a1fba7ce9e6baece53dd8e4e147171681b0f6de1cb107ca4dde9ce4f47ce96f8abf48f773b54f3a950800024329c170fbb6a150db52e973833627634d61cf6f67fb5c59a993b145f5d5c81da10a8d3b4054f2e55434c683ddbd7f929e9a9608e6be8c432faf9ced61aa47b4a165bbcb74961be8bed0abca715365ce305af4733100010001211b0c33353cdf7a320f768e3dc40bce1326d639fcac099bba9ecd8e34eaf192d25f4ef91464429e7230120bfa4208903f9126cda586bae576f470666ea19cdd75c874579afee6be7a2ae0287baa91549be3c9dfcfbadbc90f83519ef7fcb108cb0e3139b09b93089c181eec46dd4bad7653a16dfa58e9a7a87f0b7c4ecc78b8106b9d89f545824fb16b1fbf16d9c14f6fba40896c133402901cf21d9906dc69b8ecedec157b3309ff69714774ff9171ea92bccd78d4f78e3d6c5dee127fe88e0e50870b77ed5e50ae09ff00000000000000000000000000000000000000




0x
0000000000000000000000000000000000000000000000000000000000000020
0000000000000000000000000000000000000000000000000000000000000061


96eacc9036b1fe0d18cdf99b70ee9fbe425e967b64527607f0df8f5a003f80a
9b6ebeee5de99b6e7a7b9e22c3e1d02eaab6c57d7b1ad51509faf9c4f2f07754101b423e4861cae828d96169ff9a4614733b6e8aa1387742bfb2de2e0a548053c6c00000000000000000000000000000000000000000000000000000000000000

96eacc9036b1fe0d18cdf99b70ee9fbe425e967b64527607f0df8f5a003f80a
9a4945dadf4a220a363f71e139f750446321ee42d728f83d77aa4155b2281c8e801b423e4861cae828d96169ff9a4614733b6e8aa1387742bfb2de2e0a548053c6c794ed6937b0681a3901608169f288a0b3096b36cc460edd07e3a5ea26eaa583b9e17a10c2f8493ab458400eceda718efa7b49acc6199b1a3fa3aa0d19e9da6cbad2ce5de30101ba66bb15317765db42358a6fbd1e3fd1cfd079f4d2d51c516fa8b5561434e3cb700c8df353f007d4d4413eb73df434d897086ec6de5437bc41c18d70e20b4d420bfd87c0b33464266ac
*/

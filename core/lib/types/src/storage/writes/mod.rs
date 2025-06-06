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

/// In VM there are two types of storage writes: Initial and Repeated.
///
/// After the first write to the key, we assign an index to it and in the future we should use
/// index instead of full key. It allows us to compress the data, as the full key would use 32 bytes,
/// and the index can be represented only as BYTES_PER_ENUMERATION_INDEX bytes.
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
    ///    entry <- enumeration_index || compressed value
    /// 2. if initial write:
    ///    entry <- blake2(bytes32(address), key) || compressed value
    ///
    /// size:
    /// - initial:  max of 65 bytes
    /// - repeated: max of 38 bytes
    /// - before:  156 bytes for each
    pub fn compress(&self) -> Vec<u8> {
        let mut comp_state_diff = match self.enumeration_index {
            0 => self.derived_key.to_vec(),
            enumeration_index if enumeration_index <= (u32::MAX as u64) => {
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

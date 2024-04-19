//! Basic types for FRI prover.

// TODO (PLA-773): Should be moved to the prover workspace.

use std::{convert::TryFrom, iter::repeat_with, str::FromStr};

use serde::{Deserialize, Serialize};

const BLOB_CHUNK_SIZE: usize = 31;
const ELEMENTS_PER_4844_BLOCK: usize = 4096;
pub const MAX_4844_BLOBS_PER_BLOCK: usize = 16;

pub const EIP_4844_BLOB_SIZE: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;

/// Wrapper struct over Vec<u8>, represents 1 EIP4844 Blob
pub type Blob = Vec<u8>;

/// EIP4844 blobs, represents all blobs in a block.
/// A block may have between 1 and 16 blobs. The absence is marked as a None.
/// This structure is not meant to be constructed directly, but through `Eip4844BlobsWrapper`.
type Eip4844Blobs = [Option<Blob>; MAX_4844_BLOBS_PER_BLOCK];

/// Wrapper struct, containing EIP 4844 blobs and enforcing invariants.
/// Current invariants:
///   - there are between [1, 16] blobs
///   - all blobs are of the same size [`EIP_4844_BLOB_SIZE`]
/// Creating a structure violating these constraints will panic.
///
/// Note: blobs are padded to fit the correct size.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Eip4844BlobsWrapper {
    blobs: Eip4844Blobs,
}

impl Eip4844BlobsWrapper {
    pub fn blobs(self) -> Eip4844Blobs {
        self.blobs
    }
}

impl Eip4844BlobsWrapper {
    pub fn encode(self) -> Vec<u8> {
        self.blobs().into_iter().flatten().flatten().collect()
    }

    pub fn decode(blobs: Vec<u8>) -> Self {
        let mut chunks: Vec<Blob> = blobs
            .chunks(EIP_4844_BLOB_SIZE)
            .map(|chunk| chunk.into())
            .collect();

        if let Some(last_chunk) = chunks.last_mut() {
            last_chunk.resize(EIP_4844_BLOB_SIZE, 0u8);
        } else {
            panic!("cannot create Eip4844Blobs, received empty pubdata");
        }

        assert!(
            chunks.len() <= MAX_4844_BLOBS_PER_BLOCK,
            "cannot create Eip4844Blobs, received too many blobs"
        );

        let blobs = chunks
            .into_iter()
            .map(Some)
            .chain(repeat_with(|| None))
            .take(MAX_4844_BLOBS_PER_BLOCK)
            .collect::<Vec<Option<Blob>>>()
            .try_into()
            .expect("must always be able to take 16 elements");
        Self { blobs }
    }
}

pub struct FinalProofIds {
    pub node_proof_ids: [u32; 13],
    pub eip_4844_proof_ids: [u32; 2],
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct CircuitIdRoundTuple {
    pub circuit_id: u8,
    pub aggregation_round: u8,
}

impl CircuitIdRoundTuple {
    pub fn new(circuit_id: u8, aggregation_round: u8) -> Self {
        Self {
            circuit_id,
            aggregation_round,
        }
    }
}

/// Represents the sequential number of the proof aggregation round.
/// Mostly used to be stored in `aggregation_round` column  in `prover_jobs` table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AggregationRound {
    BasicCircuits = 0,
    LeafAggregation = 1,
    NodeAggregation = 2,
    RecursionTip = 3,
    Scheduler = 4,
}

impl From<u8> for AggregationRound {
    fn from(item: u8) -> Self {
        match item {
            0 => AggregationRound::BasicCircuits,
            1 => AggregationRound::LeafAggregation,
            2 => AggregationRound::NodeAggregation,
            3 => AggregationRound::RecursionTip,
            4 => AggregationRound::Scheduler,
            _ => panic!("Invalid round"),
        }
    }
}

impl AggregationRound {
    pub fn next(&self) -> Option<AggregationRound> {
        match self {
            AggregationRound::BasicCircuits => Some(AggregationRound::LeafAggregation),
            AggregationRound::LeafAggregation => Some(AggregationRound::NodeAggregation),
            AggregationRound::NodeAggregation => Some(AggregationRound::RecursionTip),
            AggregationRound::RecursionTip => Some(AggregationRound::Scheduler),
            AggregationRound::Scheduler => None,
        }
    }
}

impl std::fmt::Display for AggregationRound {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::BasicCircuits => "basic_circuits",
            Self::LeafAggregation => "leaf_aggregation",
            Self::NodeAggregation => "node_aggregation",
            Self::RecursionTip => "recursion_tip",
            Self::Scheduler => "scheduler",
        })
    }
}

impl FromStr for AggregationRound {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "basic_circuits" => Ok(AggregationRound::BasicCircuits),
            "leaf_aggregation" => Ok(AggregationRound::LeafAggregation),
            "node_aggregation" => Ok(AggregationRound::NodeAggregation),
            "recursion_tip" => Ok(AggregationRound::RecursionTip),
            "scheduler" => Ok(AggregationRound::Scheduler),
            other => Err(format!(
                "{} is not a valid round name for witness generation",
                other
            )),
        }
    }
}

impl TryFrom<i32> for AggregationRound {
    type Error = ();

    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            x if x == AggregationRound::BasicCircuits as i32 => Ok(AggregationRound::BasicCircuits),
            x if x == AggregationRound::LeafAggregation as i32 => {
                Ok(AggregationRound::LeafAggregation)
            }
            x if x == AggregationRound::NodeAggregation as i32 => {
                Ok(AggregationRound::NodeAggregation)
            }
            x if x == AggregationRound::RecursionTip as i32 => Ok(AggregationRound::RecursionTip),
            x if x == AggregationRound::Scheduler as i32 => Ok(AggregationRound::Scheduler),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn test_eip_4844_blobs_wrapper_empty_pubdata() {
        let payload = vec![];
        let _eip_4844_blobs_wrapper = Eip4844BlobsWrapper::decode(payload);
    }

    #[test]
    #[should_panic]
    fn test_eip_4844_blobs_wrapper_too_much_pubdata() {
        // blob size (126976) * 16 (max number of blobs) + 1
        let payload = vec![1; 2031617];
        let _eip_4844_blobs_wrapper = Eip4844BlobsWrapper::decode(payload);
    }

    // General test.
    // It first creates wrappers of all possible size [1..16], missing only 1 byte for a full last blob.
    // Then it checks that all blobs are filled, but the last one.
    // Additional sanity check at the end ensures that the rest of the structure contains Nones, if no blobs were provided.
    #[test]
    fn test_eip_4844_blobs_wrapper_needs_padding() {
        for no_blobs in 1..=16 {
            let payload = vec![1; no_blobs * 126976 - 1];
            let eip_4844_blobs_wrapper = Eip4844BlobsWrapper::decode(payload);
            let blobs = eip_4844_blobs_wrapper.blobs();
            assert_eq!(blobs.len(), 16, "expecting 16 blobs, got {}", blobs.len());
            for index in 0..no_blobs - 1 {
                let blob = blobs[index]
                    .clone()
                    .expect("blob missing, although payload was provided");
                assert_eq!(
                    blob[blob.len() - 1],
                    1,
                    "blob[{}] was padded whilst it was not expecting any padding",
                    index
                );
                assert_eq!(blob[0], 1, "blob[{}]'s first byte got overwritten", index);
            }
            let blob = blobs[no_blobs - 1]
                .clone()
                .expect("last blob missing, although payload was provided");
            assert_eq!(
                blob[blob.len() - 1],
                0,
                "last blob was not padded whilst it was expecting padding"
            );
            for index in no_blobs..16 {
                assert!(blobs[index].is_none(), "blob[{}] was not None", index);
            }
        }
    }

    // General test.
    // It first creates wrappers of all possible size [1..16], filled to the last blob.
    // Then it checks that all blobs are filled as expected.
    // Additional sanity check at the end ensures that the rest of the structure contains Nones, if no blobs were provided.
    // The only difference from the previous test is that the last blob is filled.
    #[test]
    fn test_eip_4844_blobs_wrapper_needs_no_padding() {
        for no_blobs in 1..=16 {
            let payload = vec![1; no_blobs * 126976];
            let eip_4844_blobs_wrapper = Eip4844BlobsWrapper::decode(payload);
            let blobs = eip_4844_blobs_wrapper.blobs();
            assert_eq!(blobs.len(), 16, "expecting 16 blobs, got {}", blobs.len());
            for index in 0..no_blobs {
                let blob = blobs[index]
                    .clone()
                    .expect("blob missing, although payload was provided");
                assert_eq!(
                    blob[blob.len() - 1],
                    1,
                    "blob[{}] was padded whilst it was not expecting any padding",
                    index
                );
                assert_eq!(blob[0], 1, "blob[{}]'s first byte got overwritten", index);
            }

            for index in no_blobs..16 {
                assert!(blobs[index].is_none(), "blob[{}] was not None", index);
            }
        }
    }

    #[test]
    fn test_eip_4844_blobs_wrapper_encode_happy_path() {
        let initial_len = 126970;
        let blob_padded_size = EIP_4844_BLOB_SIZE;
        let payload = vec![1; initial_len];
        let eip_4844_blobs_wrapper = Eip4844BlobsWrapper::decode(payload);
        let raw = eip_4844_blobs_wrapper.encode();
        assert_ne!(raw.len(), initial_len);
        assert_eq!(raw.len(), 126976);
        for byte in raw.iter().rev().take(blob_padded_size - initial_len) {
            assert_eq!(*byte, 0);
        }
        assert_eq!(raw[0], 1);
    }
}

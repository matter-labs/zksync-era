//! Basic types for FRI prover.

// TODO (PLA-773): Should be moved to the prover workspace.

use std::{convert::TryFrom, str::FromStr};

use serde::{Deserialize, Serialize};

const BLOB_CHUNK_SIZE: usize = 31;
const ELEMENTS_PER_4844_BLOCK: usize = 4096;
pub const MAX_4844_BLOBS_PER_BLOCK: usize = 2;

pub const EIP_4844_BLOB_SIZE: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;

/// Wrapper struct over Vec<u8>, represents 1 EIP4844 Blob
pub type Blob = Vec<u8>;

/// Wrapper struct, containing EIP 4844 blobs and enforcing their invariants.
/// Current invariants:
///   - there are between [1, 2] blobs
///   - all blobs are of the same size [`EIP_4844_BLOB_SIZE`]
/// Creating a structure violating these constraints will panic.
///
/// Note: blobs are padded to fit the correct size.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Eip4844Blobs {
    blobs: Vec<Blob>,
}

impl Eip4844Blobs {
    pub fn blobs(self) -> Vec<Blob> {
        self.blobs
    }
}

impl From<Vec<u8>> for Eip4844Blobs {
    fn from(blobs: Vec<u8>) -> Self {
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

        Self { blobs: chunks }
    }
}

impl From<Eip4844Blobs> for Vec<u8> {
    fn from(eip_4844_blobs: Eip4844Blobs) -> Self {
        eip_4844_blobs.blobs.iter().flatten().copied().collect()
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
    Scheduler = 3,
}

impl From<u8> for AggregationRound {
    fn from(item: u8) -> Self {
        match item {
            0 => AggregationRound::BasicCircuits,
            1 => AggregationRound::LeafAggregation,
            2 => AggregationRound::NodeAggregation,
            3 => AggregationRound::Scheduler,
            _ => panic!("Invalid round"),
        }
    }
}

impl AggregationRound {
    pub fn next(&self) -> Option<AggregationRound> {
        match self {
            AggregationRound::BasicCircuits => Some(AggregationRound::LeafAggregation),
            AggregationRound::LeafAggregation => Some(AggregationRound::NodeAggregation),
            AggregationRound::NodeAggregation => Some(AggregationRound::Scheduler),
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
    fn test_eip_4844_blobs_empty_pubdata() {
        let payload = vec![];
        let _eip_4844_blobs: Eip4844Blobs = payload.into();
    }

    #[test]
    fn test_eip_4844_blobs_less_than_a_blob() {
        // blob size (126976) - 1
        let payload = vec![1; 126975];
        let eip_4844_blobs: Eip4844Blobs = payload.into();
        let blobs = eip_4844_blobs.blobs();
        assert_eq!(blobs.len(), 1);
        let blob = &blobs[0];
        assert_eq!(blob[blob.len() - 1], 0);
        assert_eq!(blob[0], 1);
    }

    #[test]
    fn test_eip_4844_blobs_exactly_a_blob() {
        // blob size (126976)
        let payload = vec![1; 126976];
        let eip_4844_blobs: Eip4844Blobs = payload.into();
        let blobs = eip_4844_blobs.blobs();
        assert_eq!(blobs.len(), 1);
        let blob = &blobs[0];
        assert_eq!(blob[blob.len() - 1], 1);
        assert_eq!(blob[0], 1);
    }

    #[test]
    fn test_eip_4844_blobs_less_than_two_blobs() {
        // blob size (126976) * 2 (max number of blobs) - 1
        let payload = vec![1; 253951];
        let eip_4844_blobs: Eip4844Blobs = payload.into();
        let blobs = eip_4844_blobs.blobs();
        assert_eq!(blobs.len(), 2);
        let first_blob = &blobs[0];
        assert_eq!(first_blob[first_blob.len() - 1], 1);
        assert_eq!(first_blob[0], 1);
        let second_blob = &blobs[1];
        assert_eq!(second_blob[second_blob.len() - 1], 0);
        assert_eq!(second_blob[0], 1);
    }

    #[test]
    fn test_eip_4844_blobs_exactly_two_blobs() {
        // blob size (126976) * 2 (max number of blobs)
        let payload = vec![1; 253952];
        let eip_4844_blobs: Eip4844Blobs = payload.into();
        let blobs = eip_4844_blobs.blobs();
        assert_eq!(blobs.len(), 2);
        let first_blob = &blobs[0];
        assert_eq!(first_blob[first_blob.len() - 1], 1);
        assert_eq!(first_blob[0], 1);
        let second_blob = &blobs[1];
        assert_eq!(second_blob[second_blob.len() - 1], 1);
        assert_eq!(second_blob[0], 1);
    }

    #[test]
    #[should_panic]
    fn test_eip_4844_blobs_more_than_two_blobs() {
        // blob size (126976) * 2 (max number of blobs) + 1
        let payload = vec![1; 253953];
        let _eip_4844_blobs: Eip4844Blobs = payload.into();
    }
}

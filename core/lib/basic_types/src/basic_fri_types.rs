//! Basic types for FRI prover.

// TODO (PLA-773): Should be moved to the prover workspace.

use std::{convert::TryFrom, str::FromStr};

use serde::{Deserialize, Serialize};

const BLOB_CHUNK_SIZE: usize = 31;
const ELEMENTS_PER_4844_BLOCK: usize = 4096;
pub const MAX_4844_BLOBS_PER_BLOCK: usize = 2;

pub const EIP_4844_BLOB_SIZE: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;

/// Wrapper struct over Vec<u8>
pub type Blob = Vec<u8>;

// TODO: add tests

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

    fn get_rounded_blob_len(blobs_len: usize) -> usize {
        let rounded_down_blob_len = blobs_len / EIP_4844_BLOB_SIZE * EIP_4844_BLOB_SIZE;

        // if there's remainder, we will need to round it up
        if blobs_len != rounded_down_blob_len {
            return rounded_down_blob_len + EIP_4844_BLOB_SIZE;
        }
        rounded_down_blob_len
    }

    fn enforce_blob_constraints(blobs_len: usize, rounded_blob_len: usize) {
        // Post calculation, rounded_blob_len should always represent full blobs; invariant
        assert!(rounded_blob_len % EIP_4844_BLOB_SIZE == 0);

        if blobs_len == 0 {
            panic!("cannot create EIP4844Blobs, received empty pubdata");
        }

        let blobs_received = rounded_blob_len / EIP_4844_BLOB_SIZE;

        if blobs_received > MAX_4844_BLOBS_PER_BLOCK {
            panic!(
                "EIP4844 supports only {:?} blobs, received {:?} (from {:?} bytes of data)",
                MAX_4844_BLOBS_PER_BLOCK, blobs_received, blobs_len
            );
        }
    }
}

impl From<Vec<u8>> for Eip4844Blobs {
    fn from(mut blobs: Vec<u8>) -> Self {
        let rounded_blob_len = Self::get_rounded_blob_len(blobs.len());
        Self::enforce_blob_constraints(blobs.len(), rounded_blob_len);

        blobs.resize(rounded_blob_len, 0u8);
        Self {
            blobs: blobs
                .chunks(EIP_4844_BLOB_SIZE)
                .map(|chunk| chunk.into())
                .collect(),
        }
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

//! Basic types for FRI prover.

// TODO (PLA-773): Should be moved to the prover workspace.

use std::{
    collections::{hash_map::IntoIter, HashMap},
    convert::TryFrom,
    iter::once,
    str::FromStr,
};

use serde::{Deserialize, Serialize};

use crate::{
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    prover_dal::JobCountStatistics,
};

const BLOB_CHUNK_SIZE: usize = 31;
const ELEMENTS_PER_4844_BLOCK: usize = 4096;
pub const MAX_4844_BLOBS_PER_BLOCK: usize = 16;

pub const EIP_4844_BLOB_SIZE: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;

/// Wrapper struct over Vec<u8>, represents 1 EIP4844 Blob
pub type Blob = Vec<u8>;

/// EIP4844 blobs, represents all blobs in a block.
/// A block may have between 1 and 16 blobs. The absence is marked as a None.
/// This structure is not meant to be constructed directly, but through `Eip4844Blobs`.
type Eip4844BlobsInner = [Option<Blob>; MAX_4844_BLOBS_PER_BLOCK];

/// External, wrapper struct, containing EIP 4844 blobs and enforcing invariants.
/// Current invariants:
///   - there are between [1, 16] blobs
///   - all blobs are of the same size [`EIP_4844_BLOB_SIZE`]
///   - there may be no blobs in case of Validium
///
/// Creating a structure violating these constraints will panic.
///
/// Note: blobs are padded to fit the correct size.
// TODO: PLA-932
/// Note 2: this becomes a rather leaky abstraction.
/// It will be reworked once `BWIP` is introduced.
/// Provers shouldn't need to decide between loading data from database or making it empty.
/// Data should just be available
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Eip4844Blobs {
    blobs: Eip4844BlobsInner,
}

impl Eip4844Blobs {
    pub fn blobs(self) -> Eip4844BlobsInner {
        self.blobs
    }
}

impl Eip4844Blobs {
    pub fn empty() -> Self {
        Self {
            blobs: Default::default(),
        }
    }

    pub fn encode(self) -> Vec<u8> {
        self.blobs().into_iter().flatten().flatten().collect()
    }

    pub fn decode(blobs: &[u8]) -> anyhow::Result<Self> {
        // Validium case
        if blobs.is_empty() {
            return Ok(Self::empty());
        }
        let mut chunks: Vec<Blob> = blobs
            .chunks(EIP_4844_BLOB_SIZE)
            .map(|chunk| chunk.into())
            .collect();
        // Unwrapping here is safe because of check on first line of the function.
        chunks.last_mut().unwrap().resize(EIP_4844_BLOB_SIZE, 0u8);

        if chunks.len() > MAX_4844_BLOBS_PER_BLOCK {
            return Err(anyhow::anyhow!(
                "cannot create Eip4844Blobs, expected max {}, received {}",
                MAX_4844_BLOBS_PER_BLOCK,
                chunks.len()
            ));
        }

        let mut blobs: [Option<Blob>; MAX_4844_BLOBS_PER_BLOCK] = Default::default();
        for (i, blob) in chunks.into_iter().enumerate() {
            blobs[i] = Some(blob);
        }

        Ok(Self { blobs })
    }
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
    pub const ALL_ROUNDS: [AggregationRound; 5] = [
        AggregationRound::BasicCircuits,
        AggregationRound::LeafAggregation,
        AggregationRound::NodeAggregation,
        AggregationRound::RecursionTip,
        AggregationRound::Scheduler,
    ];

    pub fn next(&self) -> Option<AggregationRound> {
        match self {
            AggregationRound::BasicCircuits => Some(AggregationRound::LeafAggregation),
            AggregationRound::LeafAggregation => Some(AggregationRound::NodeAggregation),
            AggregationRound::NodeAggregation => Some(AggregationRound::RecursionTip),
            AggregationRound::RecursionTip => Some(AggregationRound::Scheduler),
            AggregationRound::Scheduler => None,
        }
    }

    /// Returns all the circuit IDs that correspond to a particular
    /// aggregation round.
    ///
    /// For example, in aggregation round 0, the circuit ids should be 1 to 15 + 255 (EIP4844).
    /// In aggregation round 1, the circuit ids should be 3 to 18.
    /// In aggregation round 2, the circuit ids should be 2.
    /// In aggregation round 3, the circuit ids should be 255.
    /// In aggregation round 4, the circuit ids should be 1.
    pub fn circuit_ids(self) -> Vec<CircuitIdRoundTuple> {
        match self {
            AggregationRound::BasicCircuits => (1..=15)
                .chain(once(255))
                .map(|circuit_id| CircuitIdRoundTuple::new(circuit_id, self as u8))
                .collect(),
            AggregationRound::LeafAggregation => (3..=18)
                .map(|circuit_id| CircuitIdRoundTuple::new(circuit_id, self as u8))
                .collect(),
            AggregationRound::NodeAggregation => vec![CircuitIdRoundTuple::new(2, self as u8)],
            AggregationRound::RecursionTip => vec![CircuitIdRoundTuple::new(255, self as u8)],
            AggregationRound::Scheduler => vec![CircuitIdRoundTuple::new(1, self as u8)],
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

/// Wrapper for mapping from protocol version to prover circuits job stats
#[derive(Debug)]
pub struct ProtocolVersionedCircuitProverStats {
    protocol_versioned_circuit_stats: HashMap<ProtocolSemanticVersion, CircuitProverStats>,
}

impl FromIterator<CircuitProverStatsEntry> for ProtocolVersionedCircuitProverStats {
    fn from_iter<I: IntoIterator<Item = CircuitProverStatsEntry>>(iter: I) -> Self {
        let mut mapping = HashMap::new();
        for entry in iter {
            let protocol_semantic_version = entry.protocol_semantic_version;
            let circuit_prover_stats: &mut CircuitProverStats =
                mapping.entry(protocol_semantic_version).or_default();
            circuit_prover_stats.add(entry.circuit_id_round_tuple, entry.job_count_statistics);
        }
        Self {
            protocol_versioned_circuit_stats: mapping,
        }
    }
}

impl IntoIterator for ProtocolVersionedCircuitProverStats {
    type Item = (ProtocolSemanticVersion, CircuitProverStats);
    type IntoIter = IntoIter<ProtocolSemanticVersion, CircuitProverStats>;

    fn into_iter(self) -> Self::IntoIter {
        self.protocol_versioned_circuit_stats.into_iter()
    }
}

/// Wrapper for mapping between circuit/aggregation round to number of such jobs (queued and in progress)
#[derive(Debug, Default)]
pub struct CircuitProverStats {
    circuits_prover_stats: HashMap<CircuitIdRoundTuple, JobCountStatistics>,
}

impl IntoIterator for CircuitProverStats {
    type Item = (CircuitIdRoundTuple, JobCountStatistics);
    type IntoIter = IntoIter<CircuitIdRoundTuple, JobCountStatistics>;

    fn into_iter(self) -> Self::IntoIter {
        self.circuits_prover_stats.into_iter()
    }
}

impl CircuitProverStats {
    fn add(
        &mut self,
        circuit_id_round_tuple: CircuitIdRoundTuple,
        job_count_statistics: JobCountStatistics,
    ) {
        let stats = self
            .circuits_prover_stats
            .entry(circuit_id_round_tuple)
            .or_default();
        stats.queued += job_count_statistics.queued;
        stats.in_progress += job_count_statistics.in_progress;
    }
}

/// DTO for communication between DAL and prover_job_monitor.
/// Represents an entry -- count (queued & in progress) of jobs (circuit_id, aggregation_round) for a given protocol version.
#[derive(Debug)]
pub struct CircuitProverStatsEntry {
    circuit_id_round_tuple: CircuitIdRoundTuple,
    protocol_semantic_version: ProtocolSemanticVersion,
    job_count_statistics: JobCountStatistics,
}

impl CircuitProverStatsEntry {
    pub fn new(
        circuit_id: i16,
        aggregation_round: i16,
        protocol_version: i32,
        protocol_version_patch: i32,
        status: &str,
        count: i64,
    ) -> Self {
        let mut queued = 0;
        let mut in_progress = 0;
        match status {
            "queued" => queued = count as usize,
            "in_progress" => in_progress = count as usize,
            _ => unreachable!("received {:?}, expected only 'queued'/'in_progress' from DB as part of query filter", status),
        };

        let job_count_statistics = JobCountStatistics {
            queued,
            in_progress,
        };
        let protocol_semantic_version = ProtocolSemanticVersion::new(
            ProtocolVersionId::try_from(protocol_version as u16)
                .expect("received protocol version is broken"),
            VersionPatch(protocol_version_patch as u32),
        );

        // BEWARE, HERE BE DRAGONS.
        // In database, the `circuit_id` stored is the circuit for which the aggregation is done,
        // not the circuit which is running.
        // There is a single node level aggregation circuit, which is circuit 2.
        // This can aggregate multiple leaf nodes (which may belong to different circuits).
        // This "conversion" is a forced hacky  way to use `circuit_id` 2 for nodes.
        // A proper fix will be later provided to solve this once new auto-scaler is in place.
        let circuit_id = if aggregation_round == 2 {
            2
        } else {
            circuit_id as u8
        };
        let circuit_id_round_tuple = CircuitIdRoundTuple::new(circuit_id, aggregation_round as u8);
        CircuitProverStatsEntry {
            circuit_id_round_tuple,
            protocol_semantic_version,
            job_count_statistics,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
pub struct JobIdentifiers {
    pub circuit_id: u8,
    pub aggregation_round: u8,
    pub protocol_version: u16,
    pub protocol_version_patch: u32,
}

impl JobIdentifiers {
    pub fn get_semantic_protocol_version(&self) -> ProtocolSemanticVersion {
        ProtocolSemanticVersion::new(
            ProtocolVersionId::try_from(self.protocol_version).unwrap(),
            VersionPatch(self.protocol_version_patch),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eip_4844_blobs_empty_pubdata() {
        let payload = vec![];
        let blobs = Eip4844Blobs::decode(&payload).unwrap();
        assert_eq!(blobs, Eip4844Blobs::empty());
    }

    #[test]
    fn test_eip_4844_blobs_too_much_pubdata() {
        // blob size (126976) * 16 (max number of blobs) + 1
        let payload = vec![1; 2031617];
        match Eip4844Blobs::decode(&payload) {
            Ok(_) => panic!("expected error, got Ok"),
            Err(e) => {
                assert_eq!(
                    e.to_string(),
                    "cannot create Eip4844Blobs, expected max 16, received 17"
                );
            }
        }
    }

    // General test.
    // It first creates wrappers of all possible size [1..16], missing only 1 byte for a full last blob.
    // Then it checks that all blobs are filled, but the last one.
    // Additional sanity check at the end ensures that the rest of the structure contains `None`s, if no blobs were provided.
    #[test]
    fn test_eip_4844_blobs_needs_padding() {
        for no_blobs in 1..=16 {
            // blob size (126976) - 1 for the last byte
            let payload = vec![1; no_blobs * 126976 - 1];
            let eip_4844_blobs = Eip4844Blobs::decode(&payload).unwrap();
            let blobs = eip_4844_blobs.blobs();
            assert_eq!(blobs.len(), 16, "expecting 16 blobs, got {}", blobs.len());
            for (index, blob) in blobs.iter().enumerate().take(no_blobs - 1) {
                let blob = blob
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
            for (index, blob) in blobs.iter().enumerate().skip(no_blobs) {
                assert!(blob.is_none(), "blob[{}] was not None", index);
            }
        }
    }

    // General test.
    // It first creates wrappers of all possible size [1..16], filled to the last blob.
    // Then it checks that all blobs are filled as expected.
    // Additional sanity check at the end ensures that the rest of the structure contains `None`s, if no blobs were provided.
    // The only difference from the previous test is that the last blob is filled.
    #[test]
    fn test_eip_4844_blobs_needs_no_padding() {
        for no_blobs in 1..=16 {
            // blob size (126976)
            let payload = vec![1; no_blobs * 126976];
            let eip_4844_blobs = Eip4844Blobs::decode(&payload).unwrap();
            let blobs = eip_4844_blobs.blobs();
            assert_eq!(blobs.len(), 16, "expecting 16 blobs, got {}", blobs.len());
            for (index, blob) in blobs.iter().enumerate().take(no_blobs) {
                let blob = blob
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

            for (index, blob) in blobs.iter().enumerate().skip(no_blobs) {
                assert!(blob.is_none(), "blob[{}] was not None", index);
            }
        }
    }

    #[test]
    fn test_eip_4844_blobs_encode_happy_path() {
        let initial_len = 126970;
        let blob_padded_size = EIP_4844_BLOB_SIZE;
        let payload = vec![1; initial_len];
        let eip_4844_blobs = Eip4844Blobs::decode(&payload).unwrap();
        let raw = eip_4844_blobs.encode();
        assert_ne!(raw.len(), initial_len);
        assert_eq!(raw.len(), 126976);
        for byte in raw.iter().rev().take(blob_padded_size - initial_len) {
            assert_eq!(*byte, 0);
        }
        assert_eq!(raw[0], 1);
    }
}

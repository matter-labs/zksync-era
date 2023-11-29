//! `WitnessGenerator` component is responsible for generating prover jobs
//! and saving artifacts needed for the next round of proof aggregation.
//!
//! That is, every aggregation round needs two sets of input:
//!  * computed proofs from the previous round
//!  * some artifacts that the witness generator of previous round(s) returns.
//!
//! There are four rounds of proofs for every block,
//! each of them starts with an invocation of `WitnessGenerator` with a corresponding `WitnessGeneratorJobType`:
//!  * `WitnessGeneratorJobType::BasicCircuits`:
//!         generates basic circuits (circuits like `Main VM` - up to 50 * 48 = 2400 circuits):
//!         input table: `basic_circuit_witness_jobs` (todo SMA-1362: will be renamed from `witness_inputs`)
//!         artifact/output table: `leaf_aggregation_jobs` (also creates job stubs in `node_aggregation_jobs` and `scheduler_aggregation_jobs`)
//!         value in `aggregation_round` field of `prover_jobs` table: 0
//!  * `WitnessGeneratorJobType::LeafAggregation`:
//!         generates leaf aggregation circuits (up to 48 circuits of type `LeafAggregation`)
//!         input table: `leaf_aggregation_jobs`
//!         artifact/output table: `node_aggregation_jobs`
//!         value in `aggregation_round` field of `prover_jobs` table: 1
//!  * `WitnessGeneratorJobType::NodeAggregation`
//!         generates one circuit of type `NodeAggregation`
//!         input table: `leaf_aggregation_jobs`
//!         value in `aggregation_round` field of `prover_jobs` table: 2
//!  * scheduler circuit
//!         generates one circuit of type `Scheduler`
//!         input table: `scheduler_witness_jobs`
//!         value in `aggregation_round` field of `prover_jobs` table: 3
//!
//! One round of prover generation consists of:
//!  * `WitnessGenerator` picks up the next `queued` job in its input table and processes it
//!         (invoking the corresponding helper function in `zkevm_test_harness` repo)
//!  * it saves the generated circuis to `prover_jobs` table and the other artifacts to its output table
//!  * the individual proofs are picked up by the provers, processed, and marked as complete.
//!  * when the last proof for this round is computed, the prover updates the row in the output table
//!    setting its status to `queued`
//!  * `WitnessGenerator` picks up such job and proceeds to the next round
//!
//! Note that the very first input table (`basic_circuit_witness_jobs` (todo SMA-1362: will be renamed from `witness_inputs`))
//! is populated by the tree (as the input artifact for the `WitnessGeneratorJobType::BasicCircuits` is the merkle proofs)

use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};

use std::{fmt, time::Duration};

use zksync_types::proofs::AggregationRound;

pub mod basic_circuits;
pub mod leaf_aggregation;
pub mod node_aggregation;
mod precalculated_merkle_paths_provider;
pub mod scheduler;
mod storage_oracle;
#[cfg(test)]
mod tests;
mod utils;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "stage", format = "wit_gen_{}")]
struct StageLabel(AggregationRound);

impl From<AggregationRound> for StageLabel {
    fn from(round: AggregationRound) -> Self {
        Self(round)
    }
}

impl fmt::Display for StageLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            AggregationRound::BasicCircuits => "basic_circuits",
            AggregationRound::LeafAggregation => "leaf_aggregation",
            AggregationRound::NodeAggregation => "node_aggregation",
            AggregationRound::Scheduler => "scheduler",
        })
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_witness_generator")]
struct WitnessGeneratorMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    processing_time: Family<StageLabel, Histogram<Duration>>,
    skipped_blocks: Counter,
    sampled_blocks: Counter,
}

#[vise::register]
static METRICS: vise::Global<WitnessGeneratorMetrics> = vise::Global::new();

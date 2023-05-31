use std::time::Instant;

use zksync_types::proofs::AggregationRound;

// use crate::witness_generator::basic_circuits;

mod precalculated_merkle_paths_provider;
mod utils;

pub mod basic_circuits;
pub mod leaf_aggregation;
pub mod node_aggregation;
pub mod scheduler;

#[cfg(test)]
mod tests;

/// `WitnessGenerator` component is responsible for generating prover jobs
/// and saving artifacts needed for the next round of proof aggregation.
///
/// That is, every aggregation round needs two sets of input:
///  * computed proofs from the previous round
///  * some artifacts that the witness generator of previous round(s) returns.
///
/// There are four rounds of proofs for every block,
/// each of them starts with an invocation of `WitnessGenerator` with a corresponding `WitnessGeneratorJobType`:
///  * `WitnessGeneratorJobType::BasicCircuits`:
///         generates basic circuits (circuits like `Main VM` - up to 50 * 48 = 2400 circuits):
///         input table: `basic_circuit_witness_jobs`
///         artifact/output table: `leaf_aggregation_jobs` (also creates job stubs in `node_aggregation_jobs` and `scheduler_aggregation_jobs`)
///         value in `aggregation_round` field of `prover_jobs` table: 0
///  * `WitnessGeneratorJobType::LeafAggregation`:
///         generates leaf aggregation circuits (up to 48 circuits of type `LeafAggregation`)
///         input table: `leaf_aggregation_jobs`
///         artifact/output table: `node_aggregation_jobs`
///         value in `aggregation_round` field of `prover_jobs` table: 1
///  * `WitnessGeneratorJobType::NodeAggregation`
///         generates one circuit of type `NodeAggregation`
///         input table: `leaf_aggregation_jobs`
///         value in `aggregation_round` field of `prover_jobs` table: 2
///  * scheduler circuit
///         generates one circuit of type `Scheduler`
///         input table: `scheduler_witness_jobs`
///         value in `aggregation_round` field of `prover_jobs` table: 3
///
/// One round of prover generation consists of:
///  * `WitnessGenerator` picks up the next `queued` job in its input table and processes it
///         (invoking the corresponding helper function in `zkevm_test_harness` repo)
///  * it saves the generated circuis to `prover_jobs` table and the other artifacts to its output table
///  * the individual proofs are picked up by the provers, processed, and marked as complete.
///  * when the last proof for this round is computed, the prover updates the row in the output table
///    setting its status to `queued`
///  * `WitnessGenerator` picks up such job and proceeds to the next round
///
/// Note that the very first input table (`basic_circuit_witness_jobs`)
/// is populated by the tree (as the input artifact for the `WitnessGeneratorJobType::BasicCircuits` is the merkle proofs)
///

fn track_witness_generation_stage(started_at: Instant, round: AggregationRound) {
    let stage = match round {
        AggregationRound::BasicCircuits => "basic_circuits",
        AggregationRound::LeafAggregation => "leaf_aggregation",
        AggregationRound::NodeAggregation => "node_aggregation",
        AggregationRound::Scheduler => "scheduler",
    };
    metrics::histogram!(
        "server.witness_generator.processing_time",
        started_at.elapsed(),
        "stage" => format!("wit_gen_{}", stage)
    );
}

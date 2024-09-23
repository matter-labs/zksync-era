# WitnessGenerator

Please read this
[doc](https://www.notion.so/matterlabs/Draft-FRI-Prover-Integration-Prover-Shadowing-c4b1373786eb43779a93118be4be5d99)
for rationale of this binary, alongside the existing one in zk-core.

The component is responsible for generating prover jobs and saving artifacts needed for the next round of proof
aggregation. That is, every aggregation round needs two sets of input:

- computed proofs from the previous round
- some artifacts that the witness generator of previous round(s) returns. There are four rounds of proofs for every
  block, each of them starts with an invocation of `{Round}WitnessGenerator` with a corresponding
  `WitnessGeneratorJobType`:

## BasicCircuitsWitnessGenerator

- generates basic circuits (circuits like `Main VM` - up to 50 \* 48 = 2400 circuits):
- input table: `basic_circuit_witness_jobs` (TODO SMA-1362: will be renamed from `witness_inputs`)
- artifact/output table: `leaf_aggregation_jobs` (also creates job stubs in `node_aggregation_jobs` and
  `scheduler_aggregation_jobs`) value in `aggregation_round` field of `prover_jobs` table: 0

## LeafAggregationWitnessGenerator

- generates leaf aggregation circuits (up to 48 circuits of type `LeafAggregation`)
- input table: `leaf_aggregation_jobs`
- artifact/output table: `node_aggregation_jobs`
- value in `aggregation_round` field of `prover_jobs` table: 1

## NodeAggregationWitnessGenerator

- generates one circuit of type `NodeAggregation`
- input table: `leaf_aggregation_jobs`
- value in `aggregation_round` field of `prover_jobs` table: 2

## SchedulerWitnessGenerator

- generates one circuit of type `Scheduler`
- input table: `scheduler_witness_jobs`
- value in `aggregation_round` field of `prover_jobs` table: 3

One round of prover generation consists of:

- `WitnessGenerator` picks up the next `queued` job in its input table and processes it (invoking the corresponding
  helper function in `zkevm_test_harness` repo)
- it saves the generated circuits to `prover_jobs` table and the other artifacts to its output table
- the individual proofs are picked up by the provers, processed, and marked as complete.
- when the last proof for this round is computed, the prover updates the row in the output table setting its status to
  `queued`
- `WitnessGenerator` picks up such job and proceeds to the next round

Note that the very first input table (`witness_inputs`) is populated by the tree (as the input artifact for the
`WitnessGeneratorJobType::BasicCircuits` is the merkle proofs)

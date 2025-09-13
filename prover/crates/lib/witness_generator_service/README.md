# Witness generator Service

This crate provides the building blocks for running Witness generator. It consists of five rounds: BasicCircuits,
LeafAggregation, NodeAggregation, RecursionTip and Scheduler. Each round has its own implementation of `JobManager` and
`ArtifactsManager`.

The primitive exported by this lib is witness_generator_runner.

The rest of the codebase simply covers the internals of creating a runner, which is an implementation of
`ProverJobProcessor`.

## Job Manager

Job Manager is responsible for getting metadata from `ConnectionPool`, preparing a job (getting artifacts from
`ObjectStore` and some verification keys from `VerificationKeyManager`) and processing the job (generating witnesses
with indermediate loading and saving proofs from `ObjectStore`).

## Artifacts Manager

Artifacts Manager is responsible for getting and saving artifacts from `ObjectStore` as well as saving results to
database.

## Witness generator Runner

Runner is tied to corresponding tables depending on a round:

- BasicCircuits - `witness_inputs_fri`
- LeafAggregation - `leaf_aggregation_witness_jobs_fri`
- NodeAggregation - `node_aggregation_witness_jobs_fri`
- RecursionTip - `recursion_tip_witness_jobs_fri`
- Scheduler - `scheduler_witness_jobs_fri`

Runner operates over following object storage buckets: `WitnessInput`, `SchedulerWitnessJobsFri`,
`LeafAggregationWitnessJobsFri`, `NodeAggregationWitnessJobsFri`, `ProverJobsFri`.

Setup data should be provided as an implementation of `VerificationKeyManager`.

### Job Picker

Interacts with the database to get a job, gets input artifacts from object store and loads some verifications keys.

### Executor

Generates witnesses, gets and saves intermediate results from object store.

### Job Saver

Persists information back to corresponding table and put the output artifacts to the corresponding object storage
bucket.

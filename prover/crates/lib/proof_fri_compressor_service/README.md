# Proof FRI Compressor Service

This crate provides the building blocks for running Proof FRI Compressor. It consists of five compression steps following one SNARK wrapper step. 

The primitive exported by this lib is proof_fri_compressor_runner.

The rest of the codebase simply covers the internals of creating a runner, which is an implementation of
`ProverJobProcessor`.

## Proof FRI Compressor Runner

Runners related to generating SNARK proof and verifying it. They are tied to
`proof_compression_jobs_fri` table and operate over `ProofsFri` object storage bucket.
Setup data should be provided as an implementation of `CompressorBlobStorage`.

### Job Picker

Interacts with the database to get a job and loads the Scheduler proof and aggregation result coords from object store.

### Executor

Generates & verifies all the compression and snark wrapper steps (on GPU).

### Job Saver

Persists information back to `proof_compression_jobs_fri` table and put the final proof to the object storage bucket.
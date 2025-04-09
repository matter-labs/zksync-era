# Circuit Prover Service

This crate provides the building blocks for running circuit provers. Circuit proving is the heaviest part of the proving
process, being both the most time intensive and resource heavy part.

The primitives exported by this lib are job runners, namely:

- light_wvg_runner
- heavy_wvg_runner
- circuit_prover_runner

The rest of the codebase simply covers the internals of creating a runner, which is an implementation of
`ProverJobProcessor`.

## Witness Vector Generator Runner

Runners related to synthesizing Witness Vector (the CPU heavy part of circuit proving). They are tied to
`prover_jobs_fri` table and operate over `ProverJobsFri` object storage bucket.

Witness Vector Generators have big gaps in resource utilization and execution times. This difference can be seen at
basic level. Few basic proofs are heavier (>2.5GB RAM & > 16s), whilst the rest are rather light. (>2.5GB RAM or >16s),
whilst all others are rather light.

In current implementation we run multiple light WVG jobs and a small amount of heavy WVG jobs in order to keep good
balance between maintaining optimal RAM usage and providing maximum throughput. `MetadataLoader` abstraction was
introduced to control loading lighter and heavier jobs at runtime. Heavier picker will try to prioritize heavy circuits.
If none are available, it falls back to light jobs in order to maximize usage.

### Job Picker

Interacts with the database to get a job (as described above) and loads the data from object store.

### Executor

Straight forward, synthesizes witness vector from circuit.

### Job Saver

If successful, will provide data to GPU circuit prover over a channel. If it fails, will mark the database as such and
will later be retried (as marked by Prover Job Monitor).

## GPU Circuit Prover

Runners related to generating the circuit proof & verifying it. They are tied to `prover_jobs_fri` table and operate
over `ProverJobs` object storage bucket.

### Job Picker

Waits on information from (multiple) WVGs sent via a channel.

### Executor

Generates & verifies the circuit proof (on GPU).

### Job Saver

Persists information back to `prover_jobs_fri` table. Note that a job is picked by WVG & finished by CP.

## Diagram

```mermaid
sequenceDiagram
    box Resources
        participant db as Database
        participant os as Object Store
    end
    box Heavy/Light Witness Vector Generator
        participant wvg_p as Job Picker
        participant wvg_e as Executor
        participant wvg_s as Job Saver
    end
    box Circuit Prover
        participant cp_p as Job Picker
        participant cp_e as Executor
        participant cp_s as Job Saver
    end
    wvg_p-->>db: Get job metadata
    wvg_p-->>os: Get circuit
    wvg_p-->>wvg_p: Get finalization hints
    wvg_p-->>wvg_e: Provide metadata & circuit
    wvg_e-->>wvg_e: Synthesize witness vector
    wvg_e-->>wvg_s: Provide metadata & witness vector & circuit
    wvg_s-->>cp_p: Provide metadata & witness vector & circuit
    cp_p-->>cp_p: Get setup data
    cp_p-->>cp_e: Provide metadata & witness vector & circuit
    cp_e-->>cp_e: Prove & verify circuit proof
    cp_e-->>cp_s: Provide metadata & proof
    cp_s-->>os: Save proof
    cp_s-->>db: Update job metadata
```

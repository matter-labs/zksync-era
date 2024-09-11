# Prover subsystem introduction

The prover subsystem consists of several binaries that perform different steps of the batch proof generation process, as
follows:

- [Prover gateway][pg]: interface between core and prover subsystems, fetches batch jobs from core, and sends batch
  proofs back to core.
- [Witness generator][wg]: component that takes batch information (tx execution/state diffs/computation results) and
  constructs witness for proof generation.
- [Witness vector generator][wvg]: component that uses witness generator output and computes witness vector (_roughly_:
  data to be fed into GPU) for circuit provers.
- [Circuit prover][p]: component that generates a circuit proof (GPU accelerated).
- [Proof compressor][pc]: component that "wraps" the generated proof so that it can be sent to L1 (GPU accelerated).

While not technically a part of the prover workspace, the following components are essential for it:

- [Proof data handler][pdh]: API on the core side which Prover gateway interacts with.
- [House keeper][hk]: Metrics exporter and job rescheduler. In it's absence, jobs would not be rescheduled and metrics
  used for autoscaling would not exist, rendering internal autoscaling infrastructure useless.

Finally, the prover workspace has several CLI tools:

- [Circuit key generator][vkg]: CLI used to generate keys required for proving.
- [Prover CLI][pcli]: CLI for observing and maintaining the production proving infrastructure.

There are core components that also participate in the proof generation process by preparing the input data, such as
[metadata calculator][mc], [commitment generator][cg], [basic witness input producer][bwip], and [protective reads
writer][prw]. We won't cover them much in these docs, but it's better to know that they exist and are important for the
prover subsystem as well.

We'll cover how the components work further in documentation.

[pg]: ../crates/bin/prover_fri_gateway/
[wg]: ../crates/bin/witness_generator/
[wvg]: ../crates/bin/witness_vector_generator/
[p]: ../crates/bin/prover_fri/
[pc]: ../crates/bin/proof_fri_compressor/
[pdh]: ../../core/node/proof_data_handler/
[hk]: ../../core/node/house_keeper/
[vkg]: ../crates/bin/prover_cli/
[pcli]: ../crates/bin/vk_setup_data_generator_server_fri/
[mc]: ../../core/node/metadata_calculator/
[cg]: ../../core/node/commitment_generator/
[bwip]: ../../core/node/vm_runner/src/impls/bwip.rs
[prw]: ../../core/node/vm_runner/src/impls/protective_reads.rs

## How it runs

Proof generation is a multi-stage process, where the initial jobs are created by the Prover gateway, and then moved by
the House Keeper until the proof is generated.

The real-life deployment of prover subsystem looks as follows:

- 1x prover gateway
- 1x house keeper
- Many witness generators
- Many witness vector generators
- Many circuit provers
- 1+ proof compressors

Currently, the proving subsystem is designed to run in GCP. In theory, it's mostly environment-agnostic, and all of the
components can be launched locally, but more work is needed to run a production system in a distributed mode outside of
GCP.

Witness generators, witness vector generators, and provers are spawned on demand based on the current system load via an
autoscaler (WIP, so not released publicly yet). They can be spawned in multiple clusters among different zones, based on
the availability of machines with required specs.

## How to develop

Different parts of the subsystem have different hardware requirement, but the aggregated summary to be able to run
everything on a single machine is as follows:

- CPU with 16+ physical cores.
- GPU with CUDA support and at least 24 GB of VRAM.
- At least 64GB of RAM.
- 200+ GB of disk space. 400+ GB is recommended for development, as `/target` directory can get quite large.

Given that the requirements are quite high, it's often more convenient developing the prover in a GCP VM rather than on
a local machine. Setting up a VM is covered further in docs.

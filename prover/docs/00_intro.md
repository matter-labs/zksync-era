# Prover subsystem introduction

The prover subsystem consists of several binaries that perform different steps of the proof generation process, such as:

- [Prover gateway][pg]: component that talks to the core subsystem, fetches jobs, and submits ready proofs.
- [Witness generator][wg]: component that turns raw batch execution info into inputs suitable for generation of ZK
  proofs.
- [Witness vector generator][wvg]: component that prepares prover data to be fed into GPU.
- [GPU prover][p]: component that actually generates a proof on GPU.
- [Proof compressor][pc]: component that "wraps" the generated proof so that it can be sent to L1.

While not technically a part of the prover workspace, the following components are essential for it:

- [Proof data handler][pdh]: API on the core side which Prover gateway interacts with.
- [House keeper][hk]: A "manager" responsible for moving jobs between stages and ensuring that the system is
  operational.

Finally, the prover workspace has several CLI tools:

- [Circuit key generator][vkg]: CLI used to generate keys required for proving.
- [Prover CLI][pcli]: CLI for observing and maintaining the production proving infrastructure.

A more detailed overview for all of the components is provided further in documentation.

[pg]: ../crates/bin/prover_fri_gateway/
[wg]: ../crates/bin/witness_generator/
[wvg]: ../crates/bin/witness_vector_generator/
[p]: ../crates/bin/prover_fri/
[pc]: ../crates/bin/proof_fri_compressor/
[pdh]: ../../core/node/proof_data_handler/
[hk]: ../../core/node/house_keeper/
[vkg]: ../crates/bin/prover_cli/
[pcli]: ../crates/bin/vk_setup_data_generator_server_fri/

## How it runs

Proof generation is a multi-stage process, where the initial jobs are created by the Prover gateway, and then moved by
the House Keeper until the proof is generated.

The real-life deployment of prover subsystem looks as follows:

- 1x prover gateway
- 1x house keeper
- Many witness generators
- Many witness vector generators
- Many GPU provers
- 1x proof compressor

Currently, the proving subsystem can only run in GCP, though it's possible to run a non-production environment locally.

Witness generators, witness vector generators, and provers are spawned on demand based on the current system load via an
autoscaler. They can be spawned in multiple clusters among different zones, based on the availability of machines with
required specs.

## How to develop

Different parts of the subsystem have different hardware requirement, but the aggregated summary to be able to run
everything on a single machine is as follows:

- CPU with 16+ physical cores.
- GPU with CUDA support and at least 24 GB of VRAM.
- At least 64GB of RAM.
- 200+ GB of disk space. 400+ GB is recommended for development, as `/target` directory can get quite large.

Given that the requirements are quite high, it's often more convenient developing the prover in a GCP VM rather than on
a local machine. Setting up a VM is covered further in docs.

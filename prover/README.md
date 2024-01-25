# Prover directory

This directory contains all the libraries and binaries related to proving of the blocks.

Directories with 'fri' suffix, are mostly used with the new proof system (Boojum).

## Components

### vk_setup_data_generator_server_fri

Set of tools to create setup keys, verification keys and verification key hashes for the circuits.

Usually run once, and then we use their outputs in multiple places in the system.

### prover_fri_gateway

Communication module between the 'main' server running the state keeper, and the proving subsystem.

### witness_generator

Creating prover jobs and saving necessary artifacts.

### prover_fri

This directory contains the main 'prover'. It can be run in two modes: either as CPU or as GPU. (controlled via 'gpu'
feature flag).

### witness_vector_generator

Only used in GPU proving mode. Prepares all the witness data using CPU, and then streams it to the prover_fri.

This is mostly used for resource efficiency (as machines with GPUs are more expensive, it allows us to run many
witness_vector_generators, that can 'share' as single gpu based prover_fri).

### proof_fri_compressor

Used as a 'last step' to compress/wrap the final FRI proof into a SNARK (to make L1 verification cheaper).

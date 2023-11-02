# Verification keys

We currently have around 20 different circuits like: Scheduler, Leaf, KeccakPrecompile etc (for the full list - look at
CircuitType enum in sync_vm repo).

Each such circuit requires a separate verification key.

This crate fulfills 2 roles:

- it has the binaries that can generate the updated versions of the keys (for example if VM code changes)
- it provides the libraries that can be used by other components that need to use these keys (for example provers) -
  behaving like a key server.

Moreover, all these keys are submitted as code within the repo in `verification_XX_key.json` files.

## zksync_verification_key_server

This is the library that can be used by other components to fetch the verification key for a given circuit (using
`get_vk_for_circuit_type` function).

## zksync_verification_key_generator

The main binary that generates verification key for given circuits. Most of the heavy lifting is done by the
`create_vk_for_padding_size_log_2` method from circuit_testing repo.

The results are written to the `verification_XX_key.json` files in the current repository.

## zksync_json_to_binary_vk_converter

Converts the local json verification keys into the binary format (and stores them in the output directory).

## zksync_commitment_generator

This tool takes the 3 commitments (one for all the basic circuits, one for node and one for leaf), computed based on the
current verification keys - and updates the contract.toml config file (which is located in etc/env/base/contracts.toml).

These commitments are later used in one of the circuit breakers - to compare their values to the commitments that L1
contract holds (so that we can 'panic' properly - if we notice that our server's commitments differ from the L1
contracts - which would result in failed L1 transactions).

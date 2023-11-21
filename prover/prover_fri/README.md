# FRI Prover

## running cpu prover

`zk f cargo +nightly-2023-08-21 run --release --bin zksync_prover_fri`

## running gpu prover(requires CUDA 12.0+)

`zk f cargo +nightly-2023-08-21 run --release --features "gpu" --bin zksync_prover_fri`

## Proving a block using CPU prover locally

Below steps can be used to prove a block on local machine using CPU prover. This is useful for debugging and testing
Machine specs:

- CPU: At least 8 physical cores
- RAM: 60GB of RAM(if you have lower RAM machine enable swap)
- Disk: 400GB of free disk

1. Install the correct nightly version using command: `rustup install nightly-2023-08-21`
2. Generate the cpu setup data (no need to regenerate if it's already there). This will consume around 300Gb of disk.
   Use these commands:

   ```markdown
   for i in {1..13}; do zk f cargo run --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i
   --is_base_layer; done

   for i in {1..15}; do zk f cargo run --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i; done
   ```

3. Initialize DB and run migrations: `zk init`

4. Override the following configuration in your `dev.env`:

   ```
   ETH_SENDER_SENDER_PROOF_SENDING_MODE=OnlyRealProofs
   ETH_SENDER_SENDER_PROOF_LOADING_MODE=FriProofFromGcs
   OBJECT_STORE_FILE_BACKED_BASE_PATH=/path/to/server/artifacts
   PROVER_OBJECT_STORE_FILE_BACKED_BASE_PATH=/path/to/prover/artifacts
   FRI_PROVER_SETUP_DATA_PATH=/path/to/above-generated/cpu-setup-data
   ```

5. Run server `zk server --components=api,eth,tree,state_keeper,housekeeper,proof_data_handler` to produce blocks to be
   proven
6. Run prover gateway to fetch blocks to be proven from server :
   `zk f cargo run --release --bin zksync_prover_fri_gateway`
7. Run 4 witness generators to generate witness for each round:

   ```
   API_PROMETHEUS_LISTENER_PORT=3116 zk f cargo run --release --bin zksync_witness_generator -- --round=basic_circuits
   API_PROMETHEUS_LISTENER_PORT=3117 zk f cargo run --release --bin zksync_witness_generator -- --round=leaf_aggregation
   API_PROMETHEUS_LISTENER_PORT=3118 zk f cargo run --release --bin zksync_witness_generator -- --round=node_aggregation
   API_PROMETHEUS_LISTENER_PORT=3119 zk f cargo run --release --bin zksync_witness_generator -- --round=scheduler
   ```

8. Run prover to perform actual proving: `zk f cargo run --release --bin zksync_prover_fri`
9. Finally, run proof compressor to compress the proof to be sent on L1:
   `zk f cargo run --release --bin zksync_proof_fri_compressor`

## Proving a block using GPU prover locally

Below steps can be used to prove a block on local machine using GPU prover, It requires Cuda 12.0 installation as
pre-requisite. This is useful for debugging and testing Machine specs:

- CPU: At least 8 physical cores
- RAM: 16GB of RAM(if you have lower RAM machine enable swap)
- Disk: 30GB of free disk
- GPU: 1x Nvidia L4/T4 with 16GB of GPU RAM

1. Install the correct nightly version using command: `rustup install nightly-2023-08-21`
2. Generate the gpu setup data (no need to regenerate if it's already there). This will consume around 300Gb of disk.
   Use these commands:

   ```markdown
   for i in {1..13}; do zk f cargo run --features "gpu" --release --bin zksync_setup_data_generator_fri --
   --numeric-circuit $i --is_base_layer done

   for i in {1..15}; do zk f cargo run --features "gpu" --release --bin zksync_setup_data_generator_fri --
   --numeric-circuit $i done
   ```

3. Initialize DB and run migrations: `zk init`

4. Override the following configuration in your `dev.env`:

   ```
   ETH_SENDER_SENDER_PROOF_SENDING_MODE=OnlyRealProofs
   ETH_SENDER_SENDER_PROOF_LOADING_MODE=FriProofFromGcs
   OBJECT_STORE_FILE_BACKED_BASE_PATH=/path/to/server/artifacts
   PROVER_OBJECT_STORE_FILE_BACKED_BASE_PATH=/path/to/prover/artifacts
   FRI_PROVER_SETUP_DATA_PATH=/path/to/above-generated/gpu-setup-data
   ```

5. Run server `zk server --components=api,eth,tree,state_keeper,housekeeper,proof_data_handler` to produce blocks to be
   proven
6. Run prover gateway to fetch blocks to be proven from server :
   `zk f cargo run --release --bin zksync_prover_fri_gateway`
7. Run 4 witness generators to generate witness for each round:

   ```
   API_PROMETHEUS_LISTENER_PORT=3116 zk f cargo run --release --bin zksync_witness_generator -- --round=basic_circuits
   API_PROMETHEUS_LISTENER_PORT=3117 zk f cargo run --release --bin zksync_witness_generator -- --round=leaf_aggregation
   API_PROMETHEUS_LISTENER_PORT=3118 zk f cargo run --release --bin zksync_witness_generator -- --round=node_aggregation
   API_PROMETHEUS_LISTENER_PORT=3119 zk f cargo run --release --bin zksync_witness_generator -- --round=scheduler
   ```

8. Run prover to perform actual proving: `zk f cargo run --features "gpu" --release --bin zksync_prover_fri`
9. Run 5 witness vector generators to feed jobs to GPU prover:

   ```
   FRI_WITNESS_VECTOR_GENERATOR_PROMETHEUS_LISTENER_PORT=3416 zk f cargo run --release --bin zksync_witness_vector_generator
   FRI_WITNESS_VECTOR_GENERATOR_PROMETHEUS_LISTENER_PORT=3417 zk f cargo run --release --bin zksync_witness_vector_generator
   FRI_WITNESS_VECTOR_GENERATOR_PROMETHEUS_LISTENER_PORT=3418 zk f cargo run --release --bin zksync_witness_vector_generator
   FRI_WITNESS_VECTOR_GENERATOR_PROMETHEUS_LISTENER_PORT=3419 zk f cargo run --release --bin zksync_witness_vector_generator
   FRI_WITNESS_VECTOR_GENERATOR_PROMETHEUS_LISTENER_PORT=3420 zk f cargo run --release --bin zksync_witness_vector_generator
   ```

10. Finally, run proof compressor to compress the proof to be sent on L1:
    `zk f cargo run --release --bin zksync_proof_fri_compressor`

## Performing circuit upgrade

Performing circuit upgrade requires crypto library to be updated and generating new setup data, verification key,
finalization hints if the circuit changes. Below steps can be used to perform circuit upgrade:

1. checkout if the circuit geometry has changed in the new version of the circuit by running the
   [workflow](https://github.com/matter-labs/zkevm_test_harness/actions/workflows/geometry-config-generator.yml) in
   harness and merge the generated PR.
2. update the relevant crypto dependencies(boojum, zkevm_circuit, harness, etc) in `Cargo.lock`, for example:
   `cargo update -p zkevm_test_harness@1.4.0`
3. prepare an PR with the updated dependencies [sample PR](https://github.com/matter-labs/zksync-2-dev/pull/2481).
4. Run the verification key
   [workflow](https://github.com/matter-labs/zksync-era/actions/workflows/fri-vk-generator.yaml) against the PR to
   generate the verification key and finalization hints for the new circuit.
5. Only once the above verification key workflow is successful, start the setup-data generation(cpu, gpu setup data
   generation can be done in parallel), this step is important, since the setup data requires the new VK, we need to
   wait for it to finish.
6. Run the cpu setup data generation
   [workflow](https://github.com/matter-labs/zksync-era/actions/workflows/fri-setup-data-generator.yml) against the PR
   to generate the cpu setup data.
7. Run the gpu setup data generation
   [workflow](https://github.com/matter-labs/zksync-era/actions/workflows/fri-gpu-setup-data-generator.yml) against the
   PR to generate the gpu setup data.
8. Once the setup data generation workflows are successful, update the PR with `setup_keys_id` id in
   [build-docker-from-tag.yml](../../.github/workflows/build-docker-from-tag.yml) and in
   [fri-gpu-prover-integration-test.yml](../../.github/workflows/fri-gpu-prover-integration-test.yml), make sure to only
   do it from `FRI prover` not old.
9. Run the GPU integration test
   [workflow](https://github.com/matter-labs/zksync-era/actions/workflows/fri-gpu-prover-integration-test.yml) against
   the PR to verify the GPU prover is working fine with new circuits.

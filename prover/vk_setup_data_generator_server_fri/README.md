# Setup data and VK generator and server

## generating setup-data for specific circuit type

`zk f cargo +nightly-2023-05-31 run --release --bin zksync_setup_data_generator_fri -- --numeric-circuit 1 --is_base_layer`

## generating GPU setup-data for specific circuit type

`zk f cargo +nightly-2023-05-31 run --features "gpu" --release --bin zksync_setup_data_generator_fri -- --numeric-circuit 1 --is_base_layer`

## Generating VK's

`cargo +nightly-2023-05-31 run --release --bin zksync_vk_generator_fri`

## generating VK commitment for existing VK's

`cargo +nightly-2023-05-31 run --release --bin zksync_commitment_generator_fri`

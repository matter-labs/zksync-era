# Setup data and VK generator and server

The SNARK VK generation requires the `CRS_FILE` environment variable to be present and point to the correct file. The
file can be downloaded from the following
[link](https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2^26.key) its also present in dir after
zk init keys/setup/setup_2^26.key

## generating setup-data for specific circuit type

`zk f cargo +nightly-2023-08-21 run --release --bin zksync_setup_data_generator_fri -- --numeric-circuit 1 --is_base_layer`

## generating GPU setup-data for specific circuit type

`zk f cargo +nightly-2023-08-21 run --features "gpu" --release --bin zksync_setup_data_generator_fri -- --numeric-circuit 1 --is_base_layer`

## Generating VK's

`cargo +nightly-2023-08-21 run --release --bin zksync_vk_generator_fri`

## generating VK commitment for existing VK's

`cargo +nightly-2023-08-21 run --release --bin zksync_commitment_generator_fri`

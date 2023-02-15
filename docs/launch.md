# Running the application

This document covers common scenarios of launching zkSync applications set locally.

## Prerequisites

Prepare dev environment prerequisites: see

[Installing dependencies](./setup-dev.md)

## Setup local dev environment

Setup:

```
zk # installs and builds zk itself
zk init
```

During the first initialization you have to download around 8 GB of setup files, this should be done once. If you have a
problem on this step of the initialization, see help for the `zk run plonk-setup` command.

If you face any other problems with the `zk init` command, go to the [Troubleshooting](##Troubleshooting) section at the
end of this file. There are solutions for some common error cases.

To completely reset the dev environment:

- Stop services:

  ```
  zk down
  ```

- Repeat the setup procedure above

If `zk init` has already been executed, and now you only need to start docker containers (e.g. after reboot), simply
launch:

```
zk up
```

## (Re)deploy db and contracts

```
zk contract redeploy
```

## Environment configurations

Env config files are held in `etc/env/`

List configurations:

```
zk env
```

Switch between configurations:

```
zk env <ENV_NAME>
```

Default configuration is `dev.env`, which is generated automatically from `dev.env.example` during `zk init` command
execution.

## Build and run server

Run server:

```
zk server
```

Server is configured using env files in `./etc/env` directory. After the first initialization, file
`./etc/env/dev.env`will be created. By default, this file is copied from the `./etc/env/dev.env.example` template.

Make sure you have environment variables set right, you can check it by running: `zk env`. You should see `* dev` in
output.

## Running server using Google cloud storage object store instead of default In memory store

Get the service_account.json file containing the GCP credentials from kubernetes secret for relevant environment(stage2/
testnet2) add that file to the default location ~/gcloud/service_account.json or update object_store.toml with the file
location

```
zk server
```

## Running prover server

Running on machine without GPU

```shell
zk f cargo +nightly run --release --bin zksync_prover
```

Running on machine with GPU

```shell
zk f cargo +nightly run --features gpu --release --bin zksync_prover
```

## Running the verification key generator

```shell
# ensure that the setup_2^26.key in the current directory, the file can be download from  https://storage.googleapis.com/universal-setup/setup_2\^26.key

# To generate all verification keys
cargo run --release --bin zksync_verification_key_generator


```

## Running the setup key generator on machine with GPU

- uncomment `"core/bin/setup_key_generator_and_server",` from root `Cargo.toml` file.
- ensure that the setup_2^26.key in the current directory, the file can be downloaded from
  <https://universal-setup.ams3.digitaloceanspaces.com/setup_2^26.key>

```shell
export BELLMAN_CUDA_DIR=$PWD
# To generate setup key for specific circuit type[0 - 17], 2 below corresponds to circuit type 2.
cargo +nightly run --features gpu --release --bin zksync_setup_key_generator -- --numeric-circuit 2
```

## Running the setup key generator on machine without GPU

- uncomment `"core/bin/setup_key_generator_and_server",` from root `Cargo.toml` file.
- ensure that the setup_2^26.key in the current directory, the file can be downloaded from
  <https://universal-setup.ams3.digitaloceanspaces.com/setup_2^26.key>

```shell
# To generate setup key for specific circuit type[0 - 17], 2 below corresponds to circuit type 2.
cargo +nightly run --release --bin zksync_setup_key_generator -- --numeric-circuit 2
```

## Generating binary verification keys for existing json verification keys

```shell
cargo run --release --bin zksync_json_to_binary_vk_converter -- -o /path/to/output-binary-vk
```

## Generating commitment for existing verification keys

```shell
cargo run --release --bin zksync_commitment_generator
```

## Running the contract verifier

```shell
# To process fixed number of jobs
cargo run --release --bin zksync_contract_verifier -- --jobs-number X

# To run until manual exit
zk contract_verifier
```

## Troubleshooting

### SSL error: certificate verify failed

**Problem**. `zk init` fails with the following error:

```
Initializing download: https://storage.googleapis.com/universal-setup/setup_2%5E20.key
SSL error: certificate verify failed
```

**Solution**. Make sure that the version of `axel` on your computer is `2.17.10` or higher.

### rmSync is not a function

**Problem**. `zk init` fails with the following error:

```
fs_1.default.rmSync is not a function
```

**Solution**. Make sure that the version of `node.js` installed on your computer is `14.14.0` or higher.

# Running the application

This document covers common scenarios for launching ZKsync applications set locally.

## Prerequisites

Prepare dev environment prerequisites: see

[Installing dependencies](./setup-dev.md)

## Setup local dev environment

Run the required containers with:

```bash
zkstack containers
```

Setup:

```bash
zkstack ecosystem init
```

To completely reset the dev environment:

- Stop services:

  ```bash
  zkstack dev clean all
  ```

- Repeat the setup procedure above

  ```bash
  zkstack ecosystem init
  ```

### Run observability stack

If you want to run [Dockprom](https://github.com/stefanprodan/dockprom/) stack (Prometheus, Grafana) alongside other
containers - add `--run-observability` parameter during initialisation.

```bash
zkstack containers --observability
```

or select `yes` when prompted during the interactive execution of the command.

That will also provision Grafana with
[era-observability](https://github.com/matter-labs/era-observability/tree/main/dashboards) dashboards. You can then
access it at `http://127.0.0.1:3000/` under credentials `admin/admin`.

> If you don't see any data displayed on the Grafana dashboards - try setting the timeframe to "Last 30 minutes". You
> will also have to have `jq` installed on your system.

## (Re)deploy db and contracts

```
zkstack dev contracts
```

## Ecosystem Configuration

Ecosystem configuration can be found in:

- `/ZkStack.yaml`
- `/configs` folder
- `/chains` folder

These files are created at ecosystem initialization `zkstack ecosystem init` and at chain initialization `zkstack chain init`.

## Build and run server

Run server:

```bash
zkstack server
```

## Running server using Google cloud storage object store instead of default In memory store

TODO: check the new way to do this

Get the service_account.json file containing the GCP credentials from kubernetes secret for relevant environment(stage2/
testnet2) add that file to the default location ~/gcloud/service_account.json or update object_store.toml with the file
location

```bash
zkstack server
```

## Running prover server

Running on machine without GPU

```bash
zkstack prover run
```

TODO: check the new way to do this

Running on machine with GPU

```bash
zk f cargo +nightly run --features gpu --release --bin zksync_prover
```

## Running the verification key generator

```bash
# ensure that the setup_2^26.key in the current directory, the file can be download from  https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2\^26.key

# To generate all verification keys
cargo run --release --bin zksync_verification_key_generator
```

## Generating binary verification keys for existing json verification keys

```bash
cargo run --release --bin zksync_json_to_binary_vk_converter -- -o /path/to/output-binary-vk
```

## Generating commitment for existing verification keys

```bash
cargo run --release --bin zksync_commitment_generator
```

## Running the contract verifier

```bash
zkstack contract-verifier run
```

## Troubleshooting

TODO

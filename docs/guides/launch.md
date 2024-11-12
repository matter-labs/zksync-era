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
  zkstack containers
  zkstack ecosystem init
  ```

### Run observability stack

If you want to run [Dockprom](https://github.com/stefanprodan/dockprom/) stack (Prometheus, Grafana) alongside other
containers - add `--observability` parameter during initialisation.

```bash
zkstack containers --observability
```

or select `yes` when prompted during the interactive execution of the command.

That will also provision Grafana with
[era-observability](https://github.com/matter-labs/era-observability/tree/main/dashboards) dashboards. You can then
access it at `http://127.0.0.1:3000/` under credentials `admin/admin`.

> If you don't see any data displayed on the Grafana dashboards - try setting the timeframe to "Last 30 minutes". You
> will also have to have `jq` installed on your system.

## Ecosystem Configuration

The ecosystem configuration is spread across multiple files and directories:

1. Root level:

   - `ZkStack.yaml`: Main configuration file for the entire ecosystem.

2. `configs/` directory:

   - `apps/`:
     - `portal_config.json`: Configuration for the portal application.
   - `contracts.yaml`: Defines smart contract settings and addresses.
   - `erc20.yaml`: Configuration for ERC20 tokens.
   - `initial_deployments.yaml`: Specifies initial ERC20 token deployments.
   - `wallets.yaml`: Contains wallet configurations.

3. `chains/<chain_name>/` directory:

   - `artifacts/`: Contains build/execution artifacts.
   - `configs/`: Chain-specific configuration files.
     - `contracts.yaml`: Chain-specific smart contract settings.
     - `external_node.yaml`: Configuration for external nodes.
     - `general.yaml`: General chain configuration.
     - `genesis.yaml`: Genesis configuration for the chain.
     - `secrets.yaml`: Secrets and private keys for the chain.
     - `wallets.yaml`: Wallet configurations for the chain.
   - `db/main/`: Database files for the chain.
   - `ZkStack.yaml`: Chain-specific ZkStack configuration.

These configuration files are automatically generated during the ecosystem initialization (`zkstack ecosystem init`) and
chain initialization (`zkstack chain init`) processes. They control various aspects of the ZKsync ecosystem, including:

- Network settings
- Smart contract deployments
- Token configurations
- Database settings
- Application/Service-specific parameters

It's important to note that while these files can be manually edited, any changes may be overwritten if the ecosystem or
chain is reinitialized. Always back up your modifications and exercise caution when making direct changes to these
files.

For specific configuration needs, it's recommended to use the appropriate `zkstack` commands or consult the
documentation for safe ways to customize your setup.

## Build and run server

Run server:

```bash
zkstack server
```

The server's configuration files can be found in `/chains/<chain_name>/configs` directory. These files are created when
running `zkstack chain init` command.

### Modifying configuration files manually

To manually modify configuration files:

1. Locate the relevant config file in `/chains/<chain_name>/configs`
2. Open the file in a text editor
3. Make necessary changes, following the existing format
4. Save the file
5. Restart the relevant services for changes to take effect:

```bash
zkstack server
```

> NOTE: Manual changes to configuration files may be overwritten if the ecosystem is reinitialized or the chain is
> reinitialized.

> WARNING: Some properties, such as ports, may require manual modification across different configuration files to
> ensure consistency and avoid conflicts.

## Running server using Google cloud storage object store instead of default In memory store

Get the `service_account.json` file containing the GCP credentials from kubernetes secret for relevant
environment(stage2/ testnet2) add that file to the default location `~/gcloud/service_account.json` or update
`object_store.toml` with the file location

```bash
zkstack prover init --bucket-base-url={url} --credentials-file={path/to/service_account.json}
```

## Running prover server

Running on a machine with GPU

```bash
zkstack prover run --component=prover
```

> NOTE: Running on machine without GPU is currently not supported by `zkstack`.

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

### Connection Refused

#### Problem

```bash
error sending request for url (http://127.0.0.1:8545/): error trying to connect: tcp connect error: Connection refused (os error 61)
```

#### Description

It appears that no containers are currently running, which is likely the reason you're encountering this error.

#### Solution

Ensure that the necessary containers have been started and are functioning correctly to resolve the issue.

```bash
zkstack containers
```

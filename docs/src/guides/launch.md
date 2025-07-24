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

### Advanced local setup with ZKsync Gateway

- Delete services from any other previous setup:

  ```bash
  zkstack dev clean all
  ```

- Start default ecosystem with default `era` chain config:

  ```bash
  zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --observability=false \
    --chain era \
    --update-submodules false
  ```

- Create additional `gateway` chain as rollup:

  ```bash
  zkstack chain create \
    --chain-name gateway \
    --chain-id 506 \
    --prover-mode no-proofs \
    --wallet-creation localhost \
    --l1-batch-commit-data-generator-mode rollup \
    --base-token-address 0x0000000000000000000000000000000000000001 \
    --base-token-price-nominator 1 \
    --base-token-price-denominator 1 \
    --set-as-default false \
    --evm-emulator false \
    --ignore-prerequisites --update-submodules false
  ```

- Initialise `gateway` chain:

  ```bash
  zkstack chain init \
    --deploy-paymaster \
    --l1-rpc-url=http://localhost:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_gateway \
    --chain gateway --update-submodules false
  ```

- Transform `gateway` chain from rollup to ZKsync Gateway settlement layer:

  ```bash
  zkstack chain gateway convert-to-gateway --chain gateway --ignore-prerequisites
  ```

- Start `gateway` chain server:

  ```bash
  mkdir zruns
  zkstack server --ignore-prerequisites --chain gateway &> ./zruns/gateway.log &
  zkstack server wait --ignore-prerequisites --verbose --chain gateway

  ```

- Migrate `era` chain to settle on `gateway`:

  ```bash
  zkstack chain gateway migrate-to-gateway --chain era --gateway-chain-name gateway
  ```

- Start `era` chain servers:

  ```bash
  zkstack server --ignore-prerequisites --chain era &> ./zruns/era.log &
  ```

- Your chains will be running on the following ports:

  ```bash
  localhost:3050 - chain era (271)
  localhost:3150 - chain gateway (506)
  ```

Any deposits or withdraws from L1 to `era` chain will go through `gateway`.

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

**Note:** The execution delay (time L1 batches must wait before execution) is now dynamically read from the ValidatorTimelock contract and is no longer configurable through these files.

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

```admonish note
Manual changes to configuration files may be overwritten if the ecosystem is reinitialized or the chain is
reinitialized.
```

```admonish warning
Some properties, such as ports, may require manual modification across different configuration files to
ensure consistency and avoid conflicts.
```

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

```admonish note
Running on machine without GPU is currently not supported by `zkstack`.
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

### `zkstack dev lint --check -t contracts` fails

#### Problem

```
zkstack dev lint --check -t contracts                                                                                                                                                                                                                                 138ms  13:13:01

┌   ZK Stack CLI
│
●  Running linters for targets: [".contracts"]
│
◒  Running contracts linter..                                                                                                                                                                                                                                                                                    │
■  Command failed to run
│
■    Status:
│      exit status: 2
│    Stdout:
│      yarn run v1.22.22
│      $ yarn lint:md && yarn lint:sol && yarn lint:ts && yarn prettier:check
│      $ markdownlint "**/*.md"
│      $ solhint "**/*.sol"
│      A new version of Solhint is available: 5.0.5
│      Please consider updating your Solhint package.
│
│      system-contracts/contracts/ContractDeployer.sol
│      171:27 warning Avoid to use tx.origin avoid-tx-origin
│
│      ✖ 1 problem (0 errors, 1 warning)
│
│      $ eslint .
│      info Visit https://yarnpkg.com/en/docs/cli/run for documentation about this command.
│      info Visit https://yarnpkg.com/en/docs/cli/run for documentation about this command.
│
│
│    Stderr:
│
│      Oops! Something went wrong! :(
│
│      ESLint: 8.57.0
│
│      EslintPluginImportResolveError: typescript with invalid interface loaded as resolver
│      Occurred while linting /home/evl/code/zksync-era/contracts/system-contracts/scripts/utils.ts:19
│      Rule: "import/default"
│      at requireResolver (/home/evl/code/zksync-era/node_modules/eslint-module-utils/resolve.js:204:17)
│      at fullResolve (/home/evl/code/zksync-era/node_modules/eslint-module-utils/resolve.js:141:22)
│      at Function.relative (/home/evl/code/zksync-era/node_modules/eslint-module-utils/resolve.js:158:10)
│      at remotePath (/home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:811:381)
│      at captureDependency (/home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:817:463)
│      at captureDependencyWithSpecifiers
│      (/home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:817:144)
│      at /home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:822:42
│      at Array.forEach (<anonymous>)
│      at ExportMap.parse (/home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:821:427)
│      at ExportMap.for (/home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:807:201)
│      error Command failed with exit code 2.
│      error Command failed with exit code 2.
│
│
■  Command failed to run: yarn --cwd contracts lint:check
│
│  Oops! Something went wrong! :(
│
│  ESLint: 8.57.0
│
│  EslintPluginImportResolveError: typescript with invalid interface loaded as resolver
│  Occurred while linting /home/evl/code/zksync-era/contracts/system-contracts/scripts/utils.ts:19
│  Rule: "import/default"
│      at requireResolver (/home/evl/code/zksync-era/node_modules/eslint-module-utils/resolve.js:204:17)
│      at fullResolve (/home/evl/code/zksync-era/node_modules/eslint-module-utils/resolve.js:141:22)
│      at Function.relative (/home/evl/code/zksync-era/node_modules/eslint-module-utils/resolve.js:158:10)
│      at remotePath (/home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:811:381)
│      at captureDependency (/home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:817:463)
│      at captureDependencyWithSpecifiers (/home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:817:144)
│      at /home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:822:42
│      at Array.forEach (<anonymous>)
│      at ExportMap.parse (/home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:821:427)
│      at ExportMap.for (/home/evl/code/zksync-era/contracts/node_modules/eslint-plugin-import/lib/ExportMap.js:807:201)
│  error Command failed with exit code 2.
│  error Command failed with exit code 2.
│
│
▲    0: Command failed to run: yarn --cwd contracts lint:check
│
└  Failed to run command
```

#### Description

`npm` setup is nested within our codebase. Contracts has it & so does the core codebase. The 2 can be incompatible,
therefore running `yarn install` in core ends up with a set of dependencies, with `yarn install` in contracts having a
different set of dependencies.

#### Solution

```
cd contracts && yarn install
```

```admonish note
This may cause integration tests to fail in core. If so, run `yarn install` inside core repo.
```

# Command-Line Help for `zk_inception`

This document contains the help content for the `zk_inception` command-line program.

**Command Overview:**

- [`zk_inception`↴](#zk_inception)
- [`zk_inception ecosystem`↴](#zk_inception-ecosystem)
- [`zk_inception ecosystem create`↴](#zk_inception-ecosystem-create)
- [`zk_inception ecosystem init`↴](#zk_inception-ecosystem-init)
- [`zk_inception ecosystem change-default-chain`↴](#zk_inception-ecosystem-change-default-chain)
- [`zk_inception ecosystem setup-observability`↴](#zk_inception-ecosystem-setup-observability)
- [`zk_inception chain`↴](#zk_inception-chain)
- [`zk_inception chain create`↴](#zk_inception-chain-create)
- [`zk_inception chain init`↴](#zk_inception-chain-init)
- [`zk_inception chain genesis`↴](#zk_inception-chain-genesis)
- [`zk_inception chain initialize-bridges`↴](#zk_inception-chain-initialize-bridges)
- [`zk_inception chain deploy-l2-contracts`↴](#zk_inception-chain-deploy-l2-contracts)
- [`zk_inception chain upgrader`↴](#zk_inception-chain-upgrader)
- [`zk_inception chain deploy-consensus-registry`↴](#zk_inception-chain-deploy-consensus-registry)
- [`zk_inception chain deploy-multicall3`↴](#zk_inception-chain-deploy-multicall3)
- [`zk_inception chain deploy-paymaster`↴](#zk_inception-chain-deploy-paymaster)
- [`zk_inception chain update-token-multiplier-setter`↴](#zk_inception-chain-update-token-multiplier-setter)
- [`zk_inception consensus set-validator-committee`↴](#zk_inception-consensus-set-validator-committee)
- [`zk_inception consensus get-validator-committee`↴](#zk_inception-consensus-get-validator-committee)
- [`zk_inception prover`↴](#zk_inception-prover)
- [`zk_inception prover init`↴](#zk_inception-prover-init)
- [`zk_inception prover setup-keys`↴](#zk_inception-prover-setup-keys)
- [`zk_inception prover run`↴](#zk_inception-prover-run)
- [`zk_inception prover init-bellman-cuda`↴](#zk_inception-prover-init-bellman-cuda)
- [`zk_inception server`↴](#zk_inception-server)
- [`zk_inception external-node`↴](#zk_inception-external-node)
- [`zk_inception external-node configs`↴](#zk_inception-external-node-configs)
- [`zk_inception external-node init`↴](#zk_inception-external-node-init)
- [`zk_inception external-node run`↴](#zk_inception-external-node-run)
- [`zk_inception containers`↴](#zk_inception-containers)
- [`zk_inception contract-verifier`↴](#zk_inception-contract-verifier)
- [`zk_inception contract-verifier run`↴](#zk_inception-contract-verifier-run)
- [`zk_inception contract-verifier init`↴](#zk_inception-contract-verifier-init)
- [`zk_inception portal`↴](#zk_inception-portal)
- [`zk_inception update`↴](#zk_inception-update)

## `zk_inception`

ZK Stack CLI is a set of tools for working with zk stack.

**Usage:** `zk_inception [OPTIONS] <COMMAND>`

###### **Subcommands:**

- `ecosystem` — Ecosystem related commands
- `chain` — Chain related commands
- `prover` — Prover related commands
- `server` — Run server
- `external-node` — External Node related commands
- `containers` — Run containers for local development
- `contract-verifier` — Run contract verifier
- `portal` — Run dapp-portal
- `update` — Update ZKsync

###### **Options:**

- `-v`, `--verbose` — Verbose mode
- `--chain <CHAIN>` — Chain to use
- `--ignore-prerequisites` — Ignores prerequisites checks

## `zk_inception ecosystem`

Ecosystem related commands

**Usage:** `zk_inception ecosystem <COMMAND>`

###### **Subcommands:**

- `create` — Create a new ecosystem and chain, setting necessary configurations for later initialization
- `init` — Initialize ecosystem and chain, deploying necessary contracts and performing on-chain operations
- `change-default-chain` — Change the default chain
- `setup-observability` — Setup observability for the ecosystem, downloading Grafana dashboards from the
  era-observability repo

## `zk_inception ecosystem create`

Create a new ecosystem and chain, setting necessary configurations for later initialization

**Usage:** `zk_inception ecosystem create [OPTIONS]`

###### **Options:**

- `--ecosystem-name <ECOSYSTEM_NAME>`
- `--l1-network <L1_NETWORK>` — L1 Network

  Possible values: `localhost`, `sepolia`, `holesky`, `mainnet`

- `--link-to-code <LINK_TO_CODE>` — Code link
- `--chain-name <CHAIN_NAME>`
- `--chain-id <CHAIN_ID>` — Chain ID
- `--prover-mode <PROVER_MODE>` — Prover options

  Possible values: `no-proofs`, `gpu`

- `--wallet-creation <WALLET_CREATION>` — Wallet options

  Possible values:

  - `localhost`: Load wallets from localhost mnemonic, they are funded for localhost env
  - `random`: Generate random wallets
  - `empty`: Generate placeholder wallets
  - `in-file`: Specify file with wallets

- `--wallet-path <WALLET_PATH>` — Wallet path
- `--l1-batch-commit-data-generator-mode <L1_BATCH_COMMIT_DATA_GENERATOR_MODE>` — Commit data generation mode

  Possible values: `rollup`, `validium`

- `--base-token-address <BASE_TOKEN_ADDRESS>` — Base token address
- `--base-token-price-nominator <BASE_TOKEN_PRICE_NOMINATOR>` — Base token nominator
- `--base-token-price-denominator <BASE_TOKEN_PRICE_DENOMINATOR>` — Base token denominator
- `--set-as-default <SET_AS_DEFAULT>` — Set as default chain

  Possible values: `true`, `false`

- `--start-containers <START_CONTAINERS>` — Start reth and postgres containers after creation

  Possible values: `true`, `false`

## `zk_inception ecosystem init`

Initialize ecosystem and chain, deploying necessary contracts and performing on-chain operations

**Usage:** `zk_inception ecosystem init [OPTIONS]`

###### **Options:**

- `--deploy-paymaster <DEPLOY_PAYMASTER>` — Deploy Paymaster contract

  Possible values: `true`, `false`

- `--deploy-erc20 <DEPLOY_ERC20>` — Deploy ERC20 contracts

  Possible values: `true`, `false`

- `--deploy-ecosystem <DEPLOY_ECOSYSTEM>` — Deploy ecosystem contracts

  Possible values: `true`, `false`

- `--ecosystem-contracts-path <ECOSYSTEM_CONTRACTS_PATH>` — Path to ecosystem contracts
- `--l1-rpc-url <L1_RPC_URL>` — L1 RPC URL
- `--verify <VERIFY>` — Verify deployed contracts

  Possible values: `true`, `false`

- `--verifier <VERIFIER>` — Verifier to use

  Default value: `etherscan`

  Possible values: `etherscan`, `sourcify`, `blockscout`, `oklink`

- `--verifier-url <VERIFIER_URL>` — Verifier URL, if using a custom provider
- `--verifier-api-key <VERIFIER_API_KEY>` — Verifier API key
- `--resume`
- `-a`, `--additional-args <ADDITIONAL_ARGS>` — List of additional arguments that can be passed through the CLI.

  e.g.: `zk_inception init -a --private-key=<PRIVATE_KEY>`

- `--server-db-url <SERVER_DB_URL>` — Server database url without database name
- `--server-db-name <SERVER_DB_NAME>` — Server database name
- `--prover-db-url <PROVER_DB_URL>` — Prover database url without database name
- `--prover-db-name <PROVER_DB_NAME>` — Prover database name
- `-u`, `--use-default` — Use default database urls and names
- `-d`, `--dont-drop`
- `--dev` — Deploy ecosystem using all defaults. Suitable for local development
- `-o`, `--observability <OBSERVABILITY>` — Enable Grafana

  Possible values: `true`, `false`

## `zk_inception ecosystem change-default-chain`

Change the default chain

**Usage:** `zk_inception ecosystem change-default-chain [NAME]`

###### **Arguments:**

- `<NAME>`

## `zk_inception ecosystem setup-observability`

Setup observability for the ecosystem, downloading Grafana dashboards from the era-observability repo

**Usage:** `zk_inception ecosystem setup-observability`

## `zk_inception chain`

Chain related commands

**Usage:** `zk_inception chain <COMMAND>`

###### **Subcommands:**

- `create` — Create a new chain, setting the necessary configurations for later initialization
- `init` — Initialize chain, deploying necessary contracts and performing on-chain operations
- `genesis` — Run server genesis
- `initialize-bridges` — Initialize bridges on l2
- `deploy-l2-contracts` — Deploy all l2 contracts
- `upgrader` — Deploy Default Upgrader
- `deploy-paymaster` — Deploy paymaster smart contract
- `update-token-multiplier-setter` — Update Token Multiplier Setter address on L1

## `zk_inception chain create`

Create a new chain, setting the necessary configurations for later initialization

**Usage:** `zk_inception chain create [OPTIONS]`

###### **Options:**

- `--chain-name <CHAIN_NAME>`
- `--chain-id <CHAIN_ID>` — Chain ID
- `--prover-mode <PROVER_MODE>` — Prover options

  Possible values: `no-proofs`, `gpu`

- `--wallet-creation <WALLET_CREATION>` — Wallet options

  Possible values:

  - `localhost`: Load wallets from localhost mnemonic, they are funded for localhost env
  - `random`: Generate random wallets
  - `empty`: Generate placeholder wallets
  - `in-file`: Specify file with wallets

- `--wallet-path <WALLET_PATH>` — Wallet path
- `--l1-batch-commit-data-generator-mode <L1_BATCH_COMMIT_DATA_GENERATOR_MODE>` — Commit data generation mode

  Possible values: `rollup`, `validium`

- `--base-token-address <BASE_TOKEN_ADDRESS>` — Base token address
- `--base-token-price-nominator <BASE_TOKEN_PRICE_NOMINATOR>` — Base token nominator
- `--base-token-price-denominator <BASE_TOKEN_PRICE_DENOMINATOR>` — Base token denominator
- `--set-as-default <SET_AS_DEFAULT>` — Set as default chain

  Possible values: `true`, `false`

## `zk_inception chain init`

Initialize chain, deploying necessary contracts and performing on-chain operations

**Usage:** `zk_inception chain init [OPTIONS]`

###### **Options:**

- `--verify <VERIFY>` — Verify deployed contracts

  Possible values: `true`, `false`

- `--verifier <VERIFIER>` — Verifier to use

  Default value: `etherscan`

  Possible values: `etherscan`, `sourcify`, `blockscout`, `oklink`

- `--verifier-url <VERIFIER_URL>` — Verifier URL, if using a custom provider
- `--verifier-api-key <VERIFIER_API_KEY>` — Verifier API key
- `--resume`
- `-a`, `--additional-args <ADDITIONAL_ARGS>` — List of additional arguments that can be passed through the CLI.

  e.g.: `zk_inception init -a --private-key=<PRIVATE_KEY>`

- `--server-db-url <SERVER_DB_URL>` — Server database url without database name
- `--server-db-name <SERVER_DB_NAME>` — Server database name
- `--prover-db-url <PROVER_DB_URL>` — Prover database url without database name
- `--prover-db-name <PROVER_DB_NAME>` — Prover database name
- `-u`, `--use-default` — Use default database urls and names
- `-d`, `--dont-drop`
- `--deploy-paymaster <DEPLOY_PAYMASTER>`

  Possible values: `true`, `false`

- `--l1-rpc-url <L1_RPC_URL>` — L1 RPC URL

## `zk_inception chain genesis`

Run server genesis

**Usage:** `zk_inception chain genesis [OPTIONS]`

###### **Options:**

- `--server-db-url <SERVER_DB_URL>` — Server database url without database name
- `--server-db-name <SERVER_DB_NAME>` — Server database name
- `--prover-db-url <PROVER_DB_URL>` — Prover database url without database name
- `--prover-db-name <PROVER_DB_NAME>` — Prover database name
- `-u`, `--use-default` — Use default database urls and names
- `-d`, `--dont-drop`

## `zk_inception chain initialize-bridges`

Initialize bridges on l2

**Usage:** `zk_inception chain initialize-bridges [OPTIONS]`

###### **Options:**

- `--verify <VERIFY>` — Verify deployed contracts

  Possible values: `true`, `false`

- `--verifier <VERIFIER>` — Verifier to use

  Default value: `etherscan`

  Possible values: `etherscan`, `sourcify`, `blockscout`, `oklink`

- `--verifier-url <VERIFIER_URL>` — Verifier URL, if using a custom provider
- `--verifier-api-key <VERIFIER_API_KEY>` — Verifier API key
- `--resume`
- `-a`, `--additional-args <ADDITIONAL_ARGS>` — List of additional arguments that can be passed through the CLI.

  e.g.: `zk_inception init -a --private-key=<PRIVATE_KEY>`

## `zk_inception chain deploy-l2-contracts`

Deploy all l2 contracts

**Usage:** `zk_inception chain deploy-l2-contracts [OPTIONS]`

###### **Options:**

- `--verify <VERIFY>` — Verify deployed contracts

  Possible values: `true`, `false`

- `--verifier <VERIFIER>` — Verifier to use

  Default value: `etherscan`

  Possible values: `etherscan`, `sourcify`, `blockscout`, `oklink`

- `--verifier-url <VERIFIER_URL>` — Verifier URL, if using a custom provider
- `--verifier-api-key <VERIFIER_API_KEY>` — Verifier API key
- `--resume`
- `-a`, `--additional-args <ADDITIONAL_ARGS>` — List of additional arguments that can be passed through the CLI.

  e.g.: `zk_inception init -a --private-key=<PRIVATE_KEY>`

## `zk_inception chain upgrader`

Deploy Default Upgrader

**Usage:** `zk_inception chain upgrader [OPTIONS]`

###### **Options:**

- `--verify <VERIFY>` — Verify deployed contracts

  Possible values: `true`, `false`

- `--verifier <VERIFIER>` — Verifier to use

  Default value: `etherscan`

  Possible values: `etherscan`, `sourcify`, `blockscout`, `oklink`

- `--verifier-url <VERIFIER_URL>` — Verifier URL, if using a custom provider
- `--verifier-api-key <VERIFIER_API_KEY>` — Verifier API key
- `--resume`
- `-a`, `--additional-args <ADDITIONAL_ARGS>` — List of additional arguments that can be passed through the CLI.

  e.g.: `zk_inception init -a --private-key=<PRIVATE_KEY>`

## `zk_inception chain deploy-consensus-registry`

Deploy Consensus Registry smart contract

**Usage:** `zk_inception chain deploy-consensus-registry`

## `zk_inception chain deploy-multicall3`

Deploy Multicall3 smart contract

**Usage:** `zk_inception chain deploy-multicall3`

## `zk_inception chain deploy-paymaster`

Deploy paymaster smart contract

**Usage:** `zk_inception chain deploy-paymaster [OPTIONS]`

###### **Options:**

- `--verify <VERIFY>` — Verify deployed contracts

  Possible values: `true`, `false`

- `--verifier <VERIFIER>` — Verifier to use

  Default value: `etherscan`

  Possible values: `etherscan`, `sourcify`, `blockscout`, `oklink`

- `--verifier-url <VERIFIER_URL>` — Verifier URL, if using a custom provider
- `--verifier-api-key <VERIFIER_API_KEY>` — Verifier API key
- `--resume`
- `-a`, `--additional-args <ADDITIONAL_ARGS>` — List of additional arguments that can be passed through the CLI.

  e.g.: `zk_inception init -a --private-key=<PRIVATE_KEY>`

## `zk_inception chain update-token-multiplier-setter`

Update Token Multiplier Setter address on L1

**Usage:** `zk_inception chain update-token-multiplier-setter [OPTIONS]`

###### **Options:**

- `--verify <VERIFY>` — Verify deployed contracts

  Possible values: `true`, `false`

- `--verifier <VERIFIER>` — Verifier to use

  Default value: `etherscan`

  Possible values: `etherscan`, `sourcify`, `blockscout`, `oklink`

- `--verifier-url <VERIFIER_URL>` — Verifier URL, if using a custom provider
- `--verifier-api-key <VERIFIER_API_KEY>` — Verifier API key
- `--resume`
- `-a`, `--additional-args <ADDITIONAL_ARGS>` — List of additional arguments that can be passed through the CLI.

  e.g.: `zk_inception init -a --private-key=<PRIVATE_KEY>`

## `zk_inception consensus`

Consensus related commands

**Usage:** `zk_inception consensus <COMMAND>`

###### **Subcommands:**

- `set-validator-committee` — Set validator committee
- `get-validator-committee` — Get validator committee

## `zk_inception consensus set-validator-committee`

Set validator committee in the consensus registry smart contract. Requires `consensus_registry` and `multicall3`
contracts to be deployed.

**Usage:** `zk_inception consensus set-validator-committee [OPTIONS]`

###### **Options:**

- `--from-file <PATH>` — Set validator committee to committee specified in yaml file at `PATH`. File format is as in
  this example:

  ```yaml
  validators:
    - key: validator:public:bls12_381:????
      pop: validator:pop:bls12_381:????
      weight: 3
    - key: validator:public:bls12_381:????
      pop: validator:pop:bls12_381:????
      weight: 1
  ```

## `zk_inception consensus get-validator-committee`

Requires `consensus_registry` and `multicall3` contracts to be deployed. Fetches validator committee from the consensus
registry contract and prints it.

**Usage:** `zk_inception consensus get-validator-committee`

## `zk_inception prover`

Prover related commands

**Usage:** `zk_inception prover <COMMAND>`

###### **Subcommands:**

- `init` — Initialize prover
- `run` — Run prover
- `init-bellman-cuda` — Initialize bellman-cuda

## `zk_inception prover init`

Initialize prover

**Usage:** `zk_inception prover init [OPTIONS]`

###### **Options:**

- `--proof-store-dir <PROOF_STORE_DIR>`
- `--bucket-base-url <BUCKET_BASE_URL>`
- `--credentials-file <CREDENTIALS_FILE>`
- `--bucket-name <BUCKET_NAME>`
- `--location <LOCATION>`
- `--project-id <PROJECT_ID>`

  Possible values: `true`, `false`

- `--bellman-cuda-dir <BELLMAN_CUDA_DIR>`
- `--bellman-cuda`

  Possible values: `true`, `false`

- `--setup-compressor-key <SETUP_COMPRESSOR_KEY>`

  Possible values: `true`, `false`

- `--setup-key-path <SETUP_KEY_PATH>`
- `--setup-database <SETUP_DATABASE>`

  Possible values: `true`, `false`

- `--prover-db-url <PROVER_DB_URL>` — Prover database url without database name
- `--prover-db-name <PROVER_DB_NAME>` — Prover database name
- `-u`, `--use-default <USE_DEFAULT>` — Use default database urls and names

  Possible values: `true`, `false`

- `-d`, `--dont-drop <DONT_DROP>`

  Possible values: `true`, `false`

- `--cloud-type <CLOUD_TYPE>`

  Possible values: `gcp`, `local`

## `zk_inception prover setup-keys`

Setup keys

**Usage:** `zk_inception prover setup-keys`

###### **Options:**

- `--mode`

  Possible valuess: `download`, `generate`

- `--region`

  Possible values: `asia`, `europe`, `us`

## `zk_inception prover run`

Run prover

**Usage:** `zk_inception prover run [OPTIONS]`

###### **Options:**

- `--component <COMPONENT>`

  Possible values: `gateway`, `witness-generator`, `witness-vector-generator`, `prover`, `compressor`,
  `prover-job-monitor`

- `--docker` - Whether to run image of the component instead of binary.

  Possible values: `true`, `false`

- `--tag' - Tag of the docker image to run.

  Default value is `latest2.0` but you can specify your prefered one.

- `--round <ROUND>`

  Possible values: `all-rounds`, `basic-circuits`, `leaf-aggregation`, `node-aggregation`, `recursion-tip`, `scheduler`

- `--threads <THREADS>`

- `--max-allocation <MAX_ALLOCATION_IN_BYTES>` - in case you are running prover component, the value limits maximum
  memory allocation of it in bytes.

## `zk_inception prover init-bellman-cuda`

Initialize bellman-cuda

**Usage:** `zk_inception prover init-bellman-cuda [OPTIONS]`

###### **Options:**

- `--bellman-cuda-dir <BELLMAN_CUDA_DIR>`

## `zk_inception server`

Run server

**Usage:** `zk_inception server [OPTIONS]`

###### **Options:**

- `--components <COMPONENTS>` — Components of server to run
- `--genesis` — Run server in genesis mode
- `-a`, `--additional-args <ADDITIONAL_ARGS>` — Additional arguments that can be passed through the CLI
- `--build` — Build server but don't run it

## `zk_inception external-node`

External Node related commands

**Usage:** `zk_inception external-node <COMMAND>`

###### **Subcommands:**

- `configs` — Prepare configs for EN
- `init` — Init databases
- `run` — Run external node

## `zk_inception external-node configs`

Prepare configs for EN

**Usage:** `zk_inception external-node configs [OPTIONS]`

###### **Options:**

- `--db-url <DB_URL>`
- `--db-name <DB_NAME>`
- `--l1-rpc-url <L1_RPC_URL>`
- `-u`, `--use-default` — Use default database urls and names

## `zk_inception external-node init`

Init databases

**Usage:** `zk_inception external-node init`

## `zk_inception external-node run`

Run external node

**Usage:** `zk_inception external-node run [OPTIONS]`

###### **Options:**

- `--reinit`
- `--components <COMPONENTS>` — Components of server to run
- `-a`, `--additional-args <ADDITIONAL_ARGS>` — Additional arguments that can be passed through the CLI

## `zk_inception containers`

Run containers for local development

**Usage:** `zk_inception containers [OPTIONS]`

###### **Options:**

- `-o`, `--observability <OBSERVABILITY>` — Enable Grafana

  Possible values: `true`, `false`

## `zk_inception contract-verifier`

Run contract verifier

**Usage:** `zk_inception contract-verifier <COMMAND>`

###### **Subcommands:**

- `run` — Run contract verifier
- `init` — Download required binaries for contract verifier

## `zk_inception contract-verifier run`

Run contract verifier

**Usage:** `zk_inception contract-verifier run`

## `zk_inception contract-verifier init`

Download required binaries for contract verifier

**Usage:** `zk_inception contract-verifier init [OPTIONS]`

###### **Options:**

- `--zksolc-version <ZKSOLC_VERSION>` — Version of zksolc to install
- `--zkvyper-version <ZKVYPER_VERSION>` — Version of zkvyper to install
- `--solc-version <SOLC_VERSION>` — Version of solc to install
- `--vyper-version <VYPER_VERSION>` — Version of vyper to install

## `zk_inception portal`

Run dapp-portal

**Usage:** `zk_inception portal [OPTIONS]`

###### **Options:**

- `--port <PORT>` — The port number for the portal app

  Default value: `3030`

## `zk_inception update`

Update ZKsync

**Usage:** `zk_inception update [OPTIONS]`

###### **Options:**

- `-c`, `--only-config` — Update only the config files

<hr/>

<small><i> This document was generated automatically by
<a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>. </i></small>

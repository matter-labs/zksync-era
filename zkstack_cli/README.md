# ZK Stack CLI

Toolkit for creating and managing ZK Stack chains. `ZK Stack CLI` facilitates the creation and management of ZK Stacks.
Commands are interactive but can also accept arguments via the command line.

### Dependencies

Follow [these instructions](https://github.com/matter-labs/zksync-era/blob/main/docs/guides/setup-dev.md) to set up
dependencies on your machine. Ignore the Environment section for now.

### Installation

You can use `zkstackup` to install and manage `zkstack`:

```bash
curl -L https://raw.githubusercontent.com/matter-labs/zksync-era/main/zkstack_cli/zkstackup/install | bash
```

Then install the most recent version with:

```bash
zkstackup
```

Or manually build from a local copy of the [ZKsync](https://github.com/matter-labs/zksync-era/) repository:

```bash
zkstackup  --local
```

This command installs `zkstack` from the current repository.

#### Manual installation

Run from the repository root:

```bash
cargo install --path zkstack_cli/crates/zkstack --force --locked
```

And make sure that `.cargo/bin` is included into `PATH`.

### Foundry Integration

Foundry is used for deploying smart contracts. Pass flags for Foundry integration with the `-a` option, e.g.,
`-a --gas-estimate-multiplier=500`.

### Ecosystem

ZK Stack allows you to create a new ecosystem or connect to an existing one. An ecosystem includes components like the
BridgeHub, shared bridges, and state transition managers.
[Learn more](https://docs.zksync.io/zk-stack/components/shared-bridges).

#### Global Config

- `--verbose`: Show all output from all commands.
- `--chain`: Use a specific chain for ecosystem operations.
- `--ignore-prerequisites`: Do not verify prerequisites. !!!WARNING!!! This option won't show errors if required tools
  for network deployment and operation are missing.

#### Create

To create a ZK Stack project, start by creating an ecosystem:

```bash
zkstack ecosystem create
```

If you choose not to start database & L1 containers after creating the ecosystem, you can later run:

```bash
zkstack containers
```

Execute subsequent commands from within the created ecosystem folder:

```bash
cd path/to/ecosystem/name
```

#### Init

If the ecosystem has never been deployed before, initialize it:

```bash
zkstack ecosystem init
```

This initializes the first ZK chain, which becomes the default. Override with `--chain <name>` if needed. For default
params, use:

```bash
zkstack ecosystem init --dev
```

If the process gets stuck, resume it with `--resume`. This flag keeps track of already sent transactions and sends new
ones with provided params.

#### Verifying Contracts

To verify contracts, use the `--verify` flag.

- `--verifier name`: Verification provider. Options: etherscan, sourcify, blockscout (default: etherscan). Note: Add "
  /api?" to the end of the Blockscout homepage explorer URL.
- `--verifier-url` url: Optional verifier URL for submitting the verification request.
- `--verifier-api-key`: Verifier API key.

#### Changing Default Chain

To change the default ZK chain:

```bash
zkstack ecosystem change-default-chain
```

IMPORTANT: Currently, you cannot use an existing ecosystem to register a new chain. This feature will be added in the
future.

#### Observability

To setup [era-observability](https://github.com/matter-labs/era-observability):

```bash
zkstack ecosystem setup-observability
```

Or run:

```bash
zkstack ecosystem init --observability
```

To start observability containers:

```bash
zkstack containers --observability
```

### ZK Chain

#### Create

The first ZK chain is generated upon ecosystem creation. Create additional chains and switch between them:

```bash
zkstack chain create
```

#### Init

Deploy contracts and initialize Zk Chain:

```bash
zkstack chain init
```

This registers the chain in the BridgeHub and deploys all necessary contracts. Manual initialization steps:

`init`: Register in BridgeHub, deploy L2 contracts, and create genesis (preferred method). `deploy-l2-contracts`: Deploy
L2 bridge and Default Upgrade Contracts. `initialize-bridges`: Deploy L2 bridge. `upgrader`: Deploy Default Upgrade
Contract. `deploy-paymaster`: Deploy paymaster. `genesis`: Run genesis after deploying contracts (preferred if deployed
by a third party).

### ZK Server

To run the chain:

```bash
zkstack server
```

You can specify the component you want to run using `--components` flag

Specify the chain with `--chain <chain_name>`.

### Prover

#### Requirements

Ensure you have installed:

- [gcloud](https://cloud.google.com/sdk/docs/install)
- [wget](https://www.gnu.org/software/wget/)
- [cmake](https://apt.kitware.com/)
- [nvcc (CUDA toolkit)](https://developer.nvidia.com/cuda-downloads)

Refer to the [prover docs](https://github.com/matter-labs/zksync-era/blob/main/prover/docs/02_setup.md) for more
information.

#### Running the Prover

Initialize the prover:

```bash
zkstack prover init
```

Run the prover:

```bash
zkstack prover run
```

Specify the prover component with `--component <component>`. Components:
`gateway, witness-generator, witness-vector-generator, prover, compressor`.

For `witness-vector-generator`, specify the number of WVG jobs with `--threads <threads>`.

For `witness-generator`, specify the round with `--round <round>`. Rounds:
`all-rounds, basic-circuits, leaf-aggregation, node-aggregation, recursion-tip, scheduler`.

### Contract Verifier

Download required binaries (`solc`, `zksolc`, `vyper`, `zkvyper`):

```bash
zkstack contract-verifier init
```

Run the contract verifier:

```bash
zkstack contract-verifier run
```

### External Node

Commands for running an external node:

#### Configs

Prepare configs:

```bash
zkstack en configs
```

This ensures no port conflicts with the main node.

#### Init

Prepare the databases:

```bash
zkstack en init
```

#### Run

Run the external node:

```bash
zkstack en run
```

### Portal

Once you have at least one chain initialized, you can run the [portal](https://github.com/matter-labs/dapp-portal) - a
web-app to bridge tokens between L1 and L2 and more:

```bash
zkstack portal
```

This command will start the dockerized portal app using configuration from `apps/portal.config.json` file inside your
ecosystem directory. You can edit this file to configure the portal app if needed. By default, portal starts on
`http://localhost:3030`, you can configure the port in `apps.yaml` file.

### Explorer

For better understanding of the blockchain data, you can use the
[explorer](https://github.com/matter-labs/block-explorer) - a web-app to view and inspect transactions, blocks,
contracts and more.

First, each chain should be initialized:

```bash
zkstack explorer init
```

This command creates a database to store explorer data and generatesdocker compose file with explorer services
(`explorer-docker-compose.yml`).

Next, for each chain you want to have an explorer, you need to start its backend services:

```bash
zkstack explorer backend --chain <chain_name>
```

This command uses previously created docker compose file to start the services (api, data fetcher, worker) required for
the explorer.

Finally, you can run the explorer app:

```bash
zkstack explorer run
```

This command will start the dockerized explorer app using configuration from `apps/explorer.config.json` file inside
your ecosystem directory. You can edit this file to configure the app if needed. By default, explorer starts on
`http://localhost:3010`, you can configure the port in `apps.yaml` file.

### Update

To update your node:

```bash
zkstack update
```

This command pulls the latest changes, syncs the general config for all chains, and raises a warning if L1 upgrades are
needed.

## Dev

The subcommand `zkstack dev` offers tools for developing ZKsync.

### Database

Commands for database manipulation:

```bash
zkstack dev db
```

Possible commands:

- `check-sqlx-data`: Check if sqlx-data.json is up to date.
- `drop`: Drop databases.
- `migrate`: Migrate databases.
- `new-migration`: Create a new migration.
- `prepare`: Prepare sqlx-data.json.
- `reset`: Reset databases.
- `setup`: Set up databases.

### Clean

Clean artifacts:

```bash
zkstack dev clean
```

Possible commands:

- `all`: Remove containers and contracts cache.
- `containers`: Remove containers and Docker volumes.
- `contracts-cache`: Remove contracts cache.

### Tests

Run ZKsync tests:

```bash
zkstack dev test
```

Possible commands:

- `integration`: Run integration tests.
- `revert`: Run revert tests.
- `recovery`: Run recovery tests.
- `upgrade`: Run upgrade tests.
- `rust`: Run unit tests.
- `l1-contracts`: Run L1 contracts tests.
- `prover`: Run prover tests.

### Snapshot Commands

Create a snapshot of the current chain:

```bash
zkstack dev snapshot create
```

### Contracts

Build contracts:

```bash
zkstack dev contracts
```

### Format

Format code:

```bash
zkstack dev fmt
```

By default, this command runs all formatters. To run a specific fomatter use the following subcommands:

- `rustfmt`: Runs `cargo fmt`.
- `prettier`: Runs `prettier`.
- `contract`: Runs `prettier` on contracts.

### Lint

Lint code:

```bash
zkstack dev lint
```

By default, this command runs the linter on all files. To target specific file types, use the `--target` option.
Supported extensions include:

- `rs`: Rust files.
- `md`: Markdown files.
- `sol`: Solidity files.
- `js`: JavaScript files.
- `ts`: TypeScript files.
- `contracts`: files in `contracts` directory.

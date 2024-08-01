# zk_toolbox

Toolkit for creating and managing ZK Stack chains.

## ZK Inception

`ZK Inception` facilitates the creation and management of ZK Stacks. Commands are interactive but can also accept
arguments via the command line.

### Dependencies

Follow [these instructions](https://github.com/matter-labs/zksync-era/blob/main/docs/guides/setup-dev.md) to set up
dependencies on your machine. Ignore the Environment section for now.

### Installation

Install `zk_inception` from Git:

```bash
cargo install --git https://github.com/matter-labs/zksync-era/ --locked zk_inception zk_supervisor --force
```

Or manually build from a local copy of the [ZkSync](https://github.com/matter-labs/zksync-era/) repository:

```bash
./bin/zkt
```

This command installs `zk_inception` and `zk_supervisor` from the current repository.

### Foundry Integration

Foundry is used for deploying smart contracts. Pass flags for Foundry integration with the `-a` option, e.g.,
`-a --gas-estimate-multiplier=500`.

### Ecosystem

ZK Stack allows you to create a new ecosystem or connect to an existing one. An ecosystem includes components like the
BridgeHub, shared bridges, and state transition managers.
[Learn more](https://docs.zksync.io/zk-stack/components/shared-bridges.html).

#### Global Config

- `--verbose`: Show all output from all commands.
- `--chain`: Use a specific chain for ecosystem operations.
- `--ignore-prerequisites`: Do not verify prerequisites. !!!WARNING!!! This option won't show errors if required tools
  for network deployment and operation are missing.

#### Create

To create a ZK Stack project, start by creating an ecosystem:

```bash
zk_inception ecosystem create
```

If you choose not to start database & L1 containers after creating the ecosystem, you can later run:

```bash
zk_inception containers
```

Execute subsequent commands from within the created ecosystem folder:

```bash
cd path/to/ecosystem/name
```

#### Init

If the ecosystem has never been deployed before, initialize it:

```bash
zk_inception ecosystem init
```

This initializes the first ZK chain, which becomes the default. Override with `--chain <name>` if needed. For default
params, use:

```bash
zk_inception ecosystem init --dev
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
zk_inception ecosystem change-default-chain
```

IMPORTANT: Currently, you cannot use an existing ecosystem to register a new chain. This feature will be added in the
future.

### ZK Chain

#### Create

The first ZK chain is generated upon ecosystem creation. Create additional chains and switch between them:

```bash
zk_inception chain create
```

#### Init

Deploy contracts and initialize Zk Chain:

```bash
zk_inception chain init
```

This registers the chain in the BridgeHub and deploys all necessary contracts. Manual initialization steps:

`init`: Register in BridgeHub, deploy L2 contracts, and create genesis (preferred method). `deploy-l2-contracts`: Deploy
L2 bridge and Default Upgrade Contracts. `initialize-bridges`: Deploy L2 bridge. `upgrader`: Deploy Default Upgrade
Contract. `deploy-paymaster`: Deploy paymaster. `genesis`: Run genesis after deploying contracts (preferred if deployed
by a third party).

### ZK Server

To run the chain:

```bash
zk_inception server
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
zk_inception prover init
```

Generate setup keys:

```bash
zk_inception prover generate-sk
```

Run the prover:

```bash
zk_inception prover run
```

Specify the prover component with `--component <component>`. Components:
`gateway, witness-generator, witness-vector-generator, prover, compressor`.

For `witness-vector-generator`, specify the number of WVG jobs with `--threads <threads>`.

For `witness-generator`, specify the round with `--round <round>`. Rounds:
`all-rounds, basic-circuits, leaf-aggregation, node-aggregation, recursion-tip, scheduler`.

### Contract Verifier

Download required binaries (`solc`, `zksolc`, `vyper`, `zkvyper`):

```bash
zk_inception contract-verifier init
```

Run the contract verifier:

```bash
zk_inception contract-verifier run
```

### External Node

Commands for running an external node:

#### Configs

Prepare configs:

```bash
zk_inception en configs
```

This ensures no port conflicts with the main node.

#### Init

Prepare the databases:

```bash
zk_inception en init
```

#### Run

Run the external node:

```bash
zk_inception en run
```

### Update

To update your node:

```bash
zk_inception update
```

This command pulls the latest changes, syncs the general config for all chains, and raises a warning if L1 upgrades are
needed.

## ZK Supervisor

Tools for developing zkSync.

### Database

Commands for database manipulation:

```bash
zk_supervisor db
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
zk_supervisor clean
```

Possible commands:

- `all`: Remove containers and contracts cache.
- `containers`: Remove containers and Docker volumes.
- `contracts-cache`: Remove contracts cache.

### Tests

Run zkSync tests:

```bash
zk_supervisor test
```

Possible commands:

- `integration`: Run integration tests.
- `revert`: Run revert tests.
- `recovery`: Run recovery tests.

### Snapshot Commands

Create a snapshot of the current chain:

```bash
zks snapshot create
```

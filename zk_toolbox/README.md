# zk_toolbox

Toolkit for creating and managing ZK Stack chains.

## ZK Inception

ZK Inception facilitates the creation and management of ZK Stacks. All commands are interactive, but you can also pass
all necessary arguments via the command line.

### Dependencies

Ensure you have followed
[these instructions](https://github.com/matter-labs/zksync-era/blob/main/docs/guides/setup-dev.md) to set up
dependencies on your machine (don't worry about the Environment section for now).

### Installation

Install zk_inception from git:

```bash
cargo install --git https://github.com/matter-labs/zksync-era/ --locked zk_inception --force
```

Manually building from a local copy of the [ZkSync](https://github.com/matter-labs/zksync-era/) repository:

```bash
cd zk_toolbox
cargo install --path ./crates/zk_inception --force --locked
```

### Foundry Integration

Foundry is utilized for deploying smart contracts. For commands related to deployment, you can pass flags for Foundry
integration.

### Ecosystem

ZK Stack allows you to either create a new ecosystem or connect to an existing one. An ecosystem includes the components
that connects all ZK chains, like the BridgeHub, the shared bridges, and state transition managers.
[Learn more](https://docs.zksync.io/zk-stack/components/shared-bridges.html).

To create a ZK Stack project, you must first create an ecosystem:

```bash
zk_inception ecosystem create
```

If you chose to not start database & L1 containers after creating the ecosystem, you can later run
`zk_inception containers`

All subsequent commands should be executed from within the ecosystem folder you created:

```bash
cd `path/to/ecosystem/name`
```

If the ecosystem has never been deployed before, initialization is required:

```bash
zk_inception ecosystem init
```

This command also initializes the first ZK chain. Note that the very first chain becomes the default one, but you can
override it with another by using the `--chain <name>` flag.

To change the default ZK chain, use:

```bash
zk_inception ecosystem change-default-chain
```

IMPORTANT: It is not yet possible to use an existing ecosystem and register a chain to it. this feature will be added in
the future.

### ZK Chain

Upon ecosystem creation, the first ZK chain is automatically generated. However, you can create additional chains and
switch between them:

```bash
zk_inception chain create
```

Once created, contracts for the ZK chain must be deployed:

```bash
zk_inception chain init
```

Initialization utilizes the ecosystem's governance to register it in the BridgeHub.

If contracts were deployed by a third party (e.g., MatterLabs), you may need to run the genesis process locally:

```bash
zk_inception chain genesis
```

This ensures proper initialization of the server.

### Zk Server

For running the chain:

```bash
zk_inception server
```

You can specify the chain you are running by providing `--chain <chain_name>` argument

### Prover

To run the prover, follow these steps:

First, initialize the prover:

```bash
zk_inception prover init
```

You can generate the setup keys with:

```bash
zk_inception prover generate-sk
```


Finally, run the prover:

```bash
zk_inception prover run
```

You can specify the prover component to run by providing `--component <component>` argument. Possible components are: `gateway, witness-generator, witness-vector-generator, prover, compressor`



# zk_toolbox

Toolkit for creating and managing ZK Stack chains.

## ZK Inception

ZK Inception facilitates the creation and management of ZK Stacks. All commands are interactive, but you can also pass
all necessary arguments via the command line.

### Foundry Integration

Foundry is utilized for deploying smart contracts. For commands related to deployment, you can pass flags for Foundry
integration.

### Ecosystem

ZK Stack allows you to either create a new ecosystem or connect to an existing one. An ecosystem includes the components
that connects all ZK chains, like the BridgeHub, the shared bridges, and state transition managers.
[Learn more](https://docs.zksync.io/zk-stack/components/shared-bridges.html).

To create a ZK Stack project, you must first create an ecosystem:

`zk_inception ecosystem create`

All subsequent commands should be executed from within the ecosystem folder.

If the ecosystem has never been deployed before, initialization is required:

`zk_inception ecosystem init`

This command also initializes the first ZK chain. Note that the very first chain becomes the default one, but you can
override it with another by using the `--chain <name>` flag.

To change the default ZK chain, use:

`zk_inception ecosystem change-default-chain`

IMPORTANT: It is not yet possible to use an existing ecosystem and register a chain to it. this feature will be added in
the future.

### ZK Chain

Upon ecosystem creation, the first ZK chain is automatically generated. However, you can create additional chains and
switch between them:

`zk_inception chain create`

Once created, contracts for the ZK chain must be deployed:

`zk_inception chain init`

Initialization utilizes the ecosystem's governance to register it in the BridgeHub.

If contracts were deployed by a third party (e.g., MatterLabs), you may need to run the genesis process locally:

`zk_inception chain genesis`

This ensures proper initialization of the server.

# zk_toolbox

Toolkit for creating and managing the ZK Stack.

## Zk Inception

Zk Inception facilitates the creation and management of ZK stacks. All commands are interactive, but you can also pass
all necessary arguments via the command line.

### Foundry Integration

Foundry is utilized for deploying smart contracts. For commands related to deployment, you can pass flags for Foundry
integration.

### Ecosystem

The Zk Stack allows you to either create a new ecosystem or connect to an existing one, including shared bridges with
state transition managers.

To create a Zk Stack project, you must first create an ecosystem:

`zk_inception ecosystem create`

All subsequent commands should be executed from within the ecosystem folder.

If the ecosystem has never been deployed before, initialization is required:

`zk_inception ecosystem init`

This command also initializes the first hyperchain. Note that the very first hyperchain becomes the default one, but you
can override it with another by using the `--hyperchain <name>` flag.

To change the default hyperchain, use:

`zk_inception ecosystem change-default-hyperchain`

### Hyperchain

Upon ecosystem creation, the first hyperchain is automatically generated. However, you can create additional hyperchains
and switch between them:

`zk_inception hyperchain create`

Once created, contracts for the hyperchain must be deployed:

`zk_inception hyperchain init`

Initialization utilizes the ecosystem's governance to register it in the shared bridge.

If contracts were deployed by a third party (e.g., MatterLabs), you may need to run the genesis process locally:

`zk_inception hyperchain genesis`

This ensures proper initialization of the server.

# Protocol Upgrade Tool

## Introduction

The Protocol Upgrade Tool is a command-line utility that enables users to upgrade the protocol of a node. It is designed
to be used in conjunction with the
[protocol upgrade proposal](https://www.notion.so/matterlabs/Server-rolling-upgrade-mechanism-e4a57f8545e84c2c9edb4928b6e0f36b).

## Usage

To generate a protocol upgrade proposal, follow the steps below:

1. Create a protocol upgrade proposal
2. Deploy new facets and generate facet cuts
3. Publish new system contracts and base system contracts
4. Prepare calldata for L2 upgrade
5. Deploy a new verifier and upgrade verifier params
6. Generate the proposal transaction and execute it

### Default Values

If not provided as arguments, the tool can retrieve certain values from environment variables or from the l1 node:

1. `l1rpc` - `ETH_CLIENT_WEB3_URL`
2. `l2rpc` - `API_WEB3_JSON_RPC_HTTP_URL`
3. `create2-address` - `CONTRACTS_CREATE2_FACTORY_ADDR`
4. `zksync-address` - `CONTRACTS_DIAMOND_PROXY_ADDR`
5. `nonce` - Taken from the node via `l1rpc`
6. `gas-price` - Taken from the node via `l1rpc`
7. `environment` - By default, set to `localhost`. Always specify it explicitly. Possible values: `localhost`,
   `testnet2`, `stage2`, `mainnet2`. Each upgrade on different environments is performed separately since the contract
   addresses differ between environments.
8. `private-key` - If not specified, the default key from the default mnemonic will be used. Always specify it
   explicitly.

### Create a Protocol Upgrade Proposal

To create a protocol upgrade proposal, use the following command:

```bash
zk f yarn start upgrades create <upgrade-name> --protocol-version <protocol-version>
```

This command will create a folder named after the upgrade in the `etc/upgrades` directory. All necessary files for the
upgrade will be generated in this folder. The folder name follows the format: `<timestamp>-<upgrade-name>`.

Subsequent commands will use the latest upgrade located in the `etc/upgrades` folder. The latest upgrade is determined
based on the timestamp in the name.

The command also creates a common file with fields such as `name`, `protocolVersion`, and `timestamp`.

### Deploy New Facets and Generate Facet Cuts

First, deploy the new facets. Later, you can generate facet cuts for them.

To deploy all facets together, use the following command:

```bash
$ zk f yarn start facets deploy-all \
--private-key <private-key> \
--l1rpc <l1rpc> \
--gas-price <gas-price> \
--nonce <nonce> \
--create2-address <create2-address> \
--zksync-address <zksync-address> \
--environment <environment>
```

This command will also generate facet cuts for all facets.

Alternatively, you can deploy facets individually using the following command:

```bash
$ zk f yarn start facets deploy \
--private-key <private-key> \
--l1rpc <l1rpc> \
--gas-price <gas-price> \
--nonce <nonce> \
--create2-address <create2-address> \
--zksync-address <zksync-address> \
--environment <environment> \
--executor \
--admin \
--getters \
--mailbox
```

The results will be saved in the `etc/upgrades/<upgrade-name>/<environment>/facets.json` file. WARNING: Redeploying
facets doesn't overwrite the `facets.json` file. You need to delete it manually.

After deploying the facets, you can generate facet cuts using the following command:

Note: `zksync-address` and `l1rpc` are required for correct generation of facet cuts, but they will be removed.

```bash
$ zk f yarn start facets generate-facet-cuts \
--l1rpc <l1rpc> \
--zksync-address <zksync-address> \
--environment <environment>
```

### Deploy New System Contracts and Base System Contracts

To publish bytecodes for new system contracts and base system contracts together, use the following command:

Note: All transactions will go through L1.

```bash
$ zk f yarn start system-contracts publish-all \
--private-key <private-key> \
--l1rpc <l1rpc> \
--l2rpc <l2rpc> \
--gas-price <gas-price> \
--nonce <nonce> \
--environment <environment>
```

Alternatively, you can deploy them individually using the following command:

```bash
$ zk f yarn start system-contracts publish \
--private-key <private-key> \
--l1rpc <l1rpc> \
--l2rpc <l2rpc> \
--gas-price <gas-price> \
--nonce <nonce> \
--environment <environment> \
--bootloader \
--default-aa \
--system-contracts
```

The results will be saved in the `etc/upgrades/<upgrade-name>/<environment>/l2Upgrade.json` file.

Please note that publishing new system contracts will append to the existing file, while publishing them all together
will overwrite the file.

### Prepare Calldata for L2 Upgrade

You can generate calldata using the Complex Upgrader contract.

- `l2-upgrader-address`: Address of the L2 upgrader contract. By default, it is the DefaultUpgrade contract from env
  `CONTRACTS_DEFAULT_UPGRADE_ADDR`.
- `use-forced-deployments`: Using this parameter, the tool will prepare calldata using the default upgrade contract abi.
  This is the preferred way to use the upgrade. If not specified, the tool will grab the data for the upgrade from the
  `delegatedCalldata` field of the L2Upgrade file.
- `use-contract-deployer`: Using this parameter, the tool will prepare calldata using the ContractDeployer contract.
  This skips the delegation step of the Complex Upgrader contract. This is mostly needed for the very first upgrade.

```bash
$ zk f yarn start l2-transaction complex-upgrader-calldata \
--environment <environment> \
--l2-upgrader-address <l2UpgraderAddress> \
--use-forced-deployments \
--use-contract-deployer
```

To generate ForceDeployment calldata for an L2 upgrade, use the following command:

```bash
$ zk f yarn start l2-transaction force-deployment-calldata \
--environment <environment>
```

### Deploy New Verifier and Upgrade Verifier Params

To deploy a new verifier, use the following command:

```bash
$ zk f yarn start crypto deploy-verifier \
--private-key <private-key> \
--l1rpc <l1rpc> \
--gas-price <gas-price> \
--nonce <nonce> \
--create2-address <create2-address> \
--environment <environment>
```

The results will be saved in the `etc/upgrades/<upgrade-name>/<environment>/crypto.json` file.

To upgrade verifier params, you can specify recursion level vk and recursion circuits set vk or use params from the
environment variables:

```bash
$ zk f yarn start crypto save-verification-params \
--recursion-node-level-vk <recursionNodeLevelVk> \
--recursion-leaf-level-vk <recursionLeafLevelVk> \
--recursion-circuits-set-vks <recursionCircuitsSetVks> \
--environment <environment>
```

The results will be saved in the `etc/upgrades/<upgrade-name>/<environment>/crypto.json` file.

### Generate Proposal Transaction and Execute It

To generate a proposal transaction, combining all the data from the previous steps and save it to the
`etc/upgrades/<upgrade-name>/<environment>/transactions.json` file, use the following command:

- `l2UpgraderAddress`: Address of the L2 upgrader contract

. By default, it's the Complex Upgrader contract. If you want to use a different contract, such as ForcedDeploy
directly, you can override it.

- `diamondUpgradeProposalId`: ID of the diamond upgrade proposal. If not specified, it will be taken from the contract
  using l1rpc and zksync-address.

```bash
$ zk f yarn start transactions build-default \
--upgrade-address <upgradeAddress> \
--upgrade-timestamp <upgradeTimestamp> \
--environment <env> \
--new-allow-list <newAllowList> \
--l2-upgrader-address <l2UpgraderAddress> \
--diamond-upgrade-proposal-id <diamondUpgradeProposalId> \
--l1rpc <l1prc> \
--zksync-address <zksyncAddress> \
--use-new-governance
```

To execute the `proposeTransparentUpgrade` transaction on L1, use the following command:

```bash
$ zk f yarn start transactions propose-upgrade \
--private-key <private-key> \
--l1rpc <l1rpc> \
--gas-price <gas-price> \
--nonce <nonce> \
--zksync-address <zksyncAddress> \
--new-governance <governanceAddress> \
--environment <environment>
```

To execute the latest upgrade, use the following command:

```bash
$ zk f yarn start transactions execute-upgrade \
--private-key <private-key> \
--l1rpc <l1rpc> \
--gas-price <gas-price> \
--nonce <nonce> \
--zksync-address <zksyncAddress> \
--new-governance <governanceAddress> \
--environment <environment>
```

To cancel the proposed upgrade, use the following command:

```bash
$ zk f yarn start transactions cancel-upgrade \
--private-key <private-key> \
--l1rpc <l1rpc> \
--zksync-address <zksyncAddress> \
--gas-price <gas-price> \
--nonce <nonce> \
--new-governance <governanceAddress> \
--environment <environment>
```

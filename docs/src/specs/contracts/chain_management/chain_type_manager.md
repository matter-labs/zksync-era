<!--- WIP --->

# Chain Type Manager (CTM)

> If someone is already familiar with the [previous version](https://github.com/code-423n4/2024-03-zksync) of ZKsync architecture, this contract was previously known as "State Transition Manager (STM)".

Currently bridging between different zk rollups requires the funds to pass through L1. This is slow & expensive.

The vision of seamless internet of value requires transfers of value to be _both_ seamless and trustless. This means that for instance different chains need to share the same L1 liquidity, i.e. a transfer of funds should never touch L1 in the process. However, it requires some sort of trust between two chains. If a malicious (or broken) rollup becomes a part of the shared liquidity pool it can steal all the funds.

However, can two instances of the same zk rollup trust each other? The answer is yes, because no new additions of rollups introduce new trust assumptions. Assuming there are no bugs in circuits, the system will work as intended.

How can two rollups know that they are two different instances of the same system? We can create a factory of such contracts (and so we would know that each new rollup created by this instance is correct one). But just creating correct contracts is not enough. Ethereum changes, new bugs may be found in the original system & so an instance that does not keep itself up-to-date with the upgrades may exploit some bug from the past and jeopardize the entire system. Just deploying is not enough. We need to constantly make sure that all chains are up to date and maintain whatever other invariants are needed for these chains to trust each other.

Let’s define as _Chain Type Manager_ (CTM) \*\*as a contract that is responsible for the following:

- It serves as a factory to deploy new ZK chains.
- It is responsible for ensuring that all the chains deployed by it are up-to-date.

Note, that this means that chains have a “weaker” governance. I.e. governance can only do very limited number of things, such as setting the validator. Chain admin can not set its own upgrades and it can only “execute” the upgrade that has already been prepared by the CTM.

In the long term vision chains deployment will be permissionless, however CTM will always remain the main point of trust and will have to be explicitly whitelisted by the decentralized governance of the entire ecosystem before its chain can get the access to the shared liquidity.

## Configurability in the current release

For now, only one CTM will be supported — the one that deploys instances of ZKsync Era, possibly using other DA layers. To read more about different DA layers, check out [this document][TODO].

The exact process of deploying & registering a chain can be [read here](./chain_genesis.md). Overall, each chain in the current release will have the following parameters:

| Chain parameter                            | Updatability   | Comment                                                                                                                                                                                                                                                                                                                                               |
| --------------------------------------- | -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| chainId                                 | Permanent      | Permanent identifier of the chain. Due to wallet support reasons, for now chainId has to be small (48 bits). This is one of the reasons why for now we’ll deploy chains manually, to prevent them from having the same chainId as some another popular chain. In the future it will be trustlessly assigned as a random 32-byte value.                       |
| baseTokenAssetId                        | Permanent      | Each chain can have their own custom base token (i.e. token used for paying the fees). It is set once during creation and can never be changed. Note, that we refer to and "asset id" here instead of an L1 address. To read more about what is assetId and how it works check out the document for [asset router](../bridging/asset_router_and_ntv/asset_router.md) |
| chainTypeManager                        | Permanent      | The CTM that deployed the chain. In principle, it could be possible to migrate between CTMs (assuming both CTMs support that). However, in practice it may be very hard and as of now such functionality is not supported.                                                                                                                               |
| admin                                   | By admin of chain | The admin of the chain. It has some limited powers to govern the chain. To read more about which powers are available to a chain admin and which precautions should be taken, check [out this document](./admin_role.md)                                                                                                               |
| validatorTimelock                       | CTM            | For now, we want all the chains to use the same 3h timelock period before their batches are finalized. Only CTM can update the address that can submit state transitions to the rollup (that is, the validatorTimelock).                                                                                                                             |
| validatorTimelock.validator             | By admin of chain | The admin of chain can choose who can submit new batches to the ValidatorTimelock.                                                                                                                                                                                                                                                                       |
| priorityTx FeeParams                    | By admin of chain | The admin of a ZK chain can amend the priority transaction fee params.                                                                                                                                                                                                                                                                                |
| transactionFilterer                     | By admin of chain | A chain may put an additional filter to the incoming L1->L2 transactions. This may be needed by a permissioned chain (e.g. a Validium bank-like corporate chain).                                                                                                                                                                                     |
| DA validation / permanent rollup status | By admin of chain | A chain can decide which DA layer to use. You check out more about [safe DA management here](./admin_role.md)                                                                                                                                                                                                                                         |
| executing upgrades                      | By admin of chain | While exclusively CTM governance can set the content of the upgrade, chains will typically be able to choose suitable time for them to actually execute it. In the current release, chains will have to follow our upgrades.                                                                                                                                |
| settlement layer                        | By admin of chain | The admin of the chain can enact migrations to other settlement layers.                                                                                                                                                                                                                                                                               |

> Note, that if we take a look at the access control for the corresponding functions inside the [AdminFacet](https://github.com/matter-labs/era-contracts/tree/8222265420f362c853da7160769620d9fed7f834/l1-contracts/contracts/state-transition/chain-deps/facets/Admin.sol), the may see that a lot of methods from above that are marked as "By admin of chain" could be in theory amended by the ChainTypeManager. However, this sort of action requires approval from decentralized governance. Also, in case of an urgent high risk situation, the decentralized governance might force upgrade the contract via CTM.

## Upgradability in the current release

In the current release, each chain will be an instance of ZKsync Era and so the upgrade process of each individual chain will be similar to that of ZKsync Era.

1. Firstly, the governance of the CTM will publish the server (including sequencer, prover, etc) that support the new version . This is done offchain. Enough time should be given to various zkStack devs to update their version.
2. The governance of the CTM will publish the upgrade onchain by automatically executing the following three transactions:

   - `setChainCreationParams` ⇒ to ensure that new chains will be created with the version
   - `setValidatorTimelock` (if needed) ⇒ to ensure that the new chains will use the new validator timelock right-away
   - `setNewVersionUpgrade` ⇒ to save the upgrade information that each chain will need to follow to conduct the upgrade on their side.

3. After that, each ChainAdmin can upgrade to the new version in suitable time for them.

> Note, that while the governance does try to give the maximal possible time for chains to upgrade, the governance will typically put restrictions (aka deadlines) on the time by which the chain has to be upgraded. If the deadline is passed, the chain can not commit new batches until the upgrade is executed.

### Emergency upgrade

In case of an emergency, the [security council](https://blog.zknation.io/introducing-zk-nation/) has the ability to freeze the ecosystem and conduct an emergency upgrade.

In case we are aware that some of the committed batches on a chain are dangerous to be executed, the CTM can call `revertBatches` on that chain. For faster reaction, the admin of the ChainTypeManager has the ability to do so without waiting for govenrnace approval that may take a lot of time. This action does not lead to funds being lost, so it is considered suitable for the partially trusted role of the admin of the ChainTypeManager.

### Issues & caveats

- If an ZK chain skips an upgrade (i.e. it has version X, it did not upgrade to `X + 1` and now the latest protocol version is `X + 2` there is no built-in way to upgrade). This team will require manual intervention from us to upgrade.
- The approach of calling `revertBatches` for malicious chains is not scalable (O(N) of the number of chains). The situation is very rare, so it is fine in the short term, but not in the long run.
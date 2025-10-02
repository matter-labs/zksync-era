<!--- WIP --->

# Safe ChainAdmin management

While the ecosystem does a [decentralized trusted governance](https://blog.zknation.io/introducing-zk-nation/), each chain has its own Chain Admin. While the upgrade parameters are chosen by the governance, chain admin is still a powerful role and should be managed carefully.

In this document we will explore what are the abilities of the ChainAdmin, how dangerous they are and how to mitigate potential issues.

## General guidelines

The system does not restrict in any way how the admin of the chain should be implemented. However special caution should be taken to keep it safe.

The general guideline is that an admin of a ZK chain should be _at least_ a well-distributed multisig. Having it as an EOA is definitely a bad idea since having this address stolen can lead to [chain being permanently frozen](#setting-da-layer).

Additional measures may be taken [to self-restrict](#proposed-modular-chainadmin-implementation) the ChainAdmin to ensure that some operations can be only done in safe fashion.

Generally all the functionality of chain admin should be treated with maximal security and caution, and having hotkey separate roles in rare circuimstances, e.g. to call `setTokenMultiplier` in case of an ERC-20 based chain.

## Chain Admin functionality

### Setting validators for a chain

The admin of a chain can call `ValidatorTimelock` on the settlement layer to add or remove validators, i.e. addresses that have the right to `commit`/`verify`/`execute` batches etc.

The system is protected against malicious validators, they can never steal funds from users. However, this role is still relatively powerful: If the DA layer is not reliable, and a batch does get executed, the funds may be frozen. This is why the chains should be [cautious about DA layers that they use](#setting-da-layer). Note, that on L1 the `ValidatorTimelock` has 3h delay, while on Gateway this timelock will not be present.

In case the malicious block has not been executed yet, it can be reverted.

### Setting DA layer

This is one of the most powerful settings that a chain can have: setting a custom DA layer. The dangers of doing this wrong are obvious: lack of proper data availability solution may lead to funds being frozen. Term "frozen funds" mainly refers to the inability to reconstruct the complete state since externally only the root hash is visible, thus preventing the chain from progressing. (Note: that funds can never be _stolen_ due to ZKP checks of the VM execution).

Sometimes, users may need assurances that a chain will never become frozen even under a malicious chain admin. A general though unstable approach is discussed [here](#proposed-modular-chainadmin-implementation), however this release comes with a solution specially tailored for rollups: the `isPermanentRollup` setting.

#### `isPermanentRollup` setting

Chain also exposes the `AdminFacet.makePermanentRollup` function. It will turn a chain into a permanent rollup, ensuring that DA validator pairs can be only set to values that are approved by decentralized governance to be used for rollups.

This functionality is obviously dangerous in a sense that it is permanent and revokes the right of the chain to change its DA layer. On the other hand, it ensures perpetual safety for users. This is the option that ZKsync Era is using.

This setting is preserved even when migrating to [gateway](../gateway/overview.md). If this setting was set while chain is on top of Gateway, and it migrates back to L1, it will keep this status, i.e. it is fully irrevocable.

### `changeFeeParams` method

This method allows to change how the fees are charged for priority operations.

The worst impact of setting this value wrongly is having L1->L2 transactions underpriced.

### `setTokenMultiplier` method

This method allows to set the token multiplier, i.e. the ratio between the price of ETH and the price of the token. It will be used for L1->L2 priority transactions.

Typically, `ChainAdmin`s of ERC20 chains will have a special hotkey responsible for calling this function to keep the price up to date. An example on how it is implemented in the current system can be seen [here](https://github.com/matter-labs/era-contracts/blob/aafee035db892689df3f7afe4b89fd6467a39313/l1-contracts/contracts/governance/ChainAdmin.sol#L23).

The worst impact of setting this value wrongly is having L1->L2 transactions underpriced.

### `setPubdataPricingMode`

This method allows to set whether the pubdata price will be taken into account for priority operations.

The worst impact of setting this value wrongly is having L1->L2 transactions underpriced.

### `setTransactionFilterer`

This method allows to set a transaction filterer, i.e. an additional validator for all incoming L1->L2 transactions. The worst impact is users' transactions being censored.

### Migration to another settlement layer

The upgrade can start migration of a chain to another settlement layer. Currently all the settlement layers are whitelisted, so generally this operation is harmless (except for the inconvenience in case the migration was unplanned).

However, some caution needs to be applied to migrate properly as described in the section below.

## Chain admin when migrating to gateway

When a chain migrates to gateway, it provides the address of the new admin on L2. The following rules apply:

- If a ZK chain has already been deployed on a settlement layer, its admin stays the same.
- If a ZK chain has not been deployed yet, then the new admin is set.

The above means that in the current release the admin of the chain on the new settlement layer is "detached" from the admin on L1. It is the responsibility of the chain to set the L2 admin correctly: either it should have the same signers or, even better in the long run, put the aliased L1 admin to have most of the abilities inside the L2 chain admin.

Since most of the Admin's functionality above are related to L1->L2 operations, the L1 chain admin will continue playing a crucial role even after the chain migrates to Gateway. However, some of the new functionality are relevant on the chain admin on the settlement layer only:

- Managing DA
- Managing new validators
- It is the admin of the settlement layer that do migrations of chains

As such, the choice of the L2 Admin is very important. Also, if the chain admin on the new settlement layer is not accessible (e.g. accidentally wrong address was chosen), the chain is lost:

- No validators will be set
- The chain can not migrate back

Overall **very special care** needs to be taken when selecting an admin for the migration to a new settlement layer.

## Proposed modular `ChainAdmin` implementation

> **Warning**. The proposed implementation here will likely **not** be used by the Matter Labs team for ZKsync Era due to the issues listed in the issues section. This code, however, is still in scope of the audit and may serve as a future basis of a more long term solution.

In order to ensure that the architecture here flexible enough for future other chains to use, it uses a modular architecture to ensure that other chains could fit it to their needs. By default, this contract is not even `Ownable`, and anyone can execute transactions out of the name of it. In order to add new features such as restricting calling dangerous methods and access control, _restrictions_ should be added there. Each restriction is a contract that implements the `IRestriction` interface. The following restrictions have been implemented so far:

- `AccessControlRestriction` that allows to specify which addresses can call which methods. In the case of Era, only the `DEFAULT_ADMIN_ROLE` will be able to call any methods. Other chains with non-ETH base token may need an account that would periodically call the L1 contract to update the ETH price there. They may create the `SET_TOKEN_MULTIPLIER_ROLE` role that is required to update the token price and give its rights to some hot private key.

- `PermanentRestriction` that ensures that:

a) This restriction could not be lifted, i.e. the chain admin of the chain must forever have it. Even if the address of the `ChainAdmin` changes, it ensures that the new admin has this restriction turned on.
b) It specifies the calldata this which certain methods can be called. For instance, in case a chain wants to keep itself permanently tied to certain DA, it will ensure that the only DA validation method that can be used is rollup. Some sort of decentralized governance could be chosen to select which DA validation pair corresponds to this DA method.

The approach above does not only helps to protect the chain, but also provides correct information for chains that are present in our ecosystem. For instance, if a chain claims to perpetually have a certain property, having the `PermanentRestriction` as part of the chain admin can ensure all observers of that.

### Issues and limitations

Due to specifics of [migration to another settlement layers](#migration-to-another-settlement-layer) (i.e. that migrations do not overwrite the admin), maintaining the same `PermanentRestriction` becomes hard in case a restriction has been added on top of the chain admin inside one chain, but not the other.

While very flexible, this modular approach should still be polished enough before recommending it as a generic solution for everyone. However, the provided new [ChainAdmin](https://github.com/matter-labs/era-contracts/blob/8222265420f362c853da7160769620d9fed7f834/l1-contracts/contracts/governance/ChainAdmin.sol) can still be helpful for new chains as with the `AccessControlRestriction` it provides a ready-to-use framework for role-based managing of the chain. Using `PermanentRestriction` for now is discouraged however.
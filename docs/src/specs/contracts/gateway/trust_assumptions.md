# Trust assumptions for settlement layers

## When settling on top of Gateway

When a chain migrates on top of a settlement layer, it necessarily puts some trust into it. While the chain settles on top of Gateway, it manages the funds that the chain has given to it via [token balance migration](../bridging/asset_tracker/asset_tracker.md#migrating-and-settling-on-gateway). While ZK proofs do ensure the correctness of the state transitions of Gateway, lack of stage1 support means that in theory a malicious chain admin of the settlement layer could block chains from withdrawing from the ZK Gateway. 

Note, that while a chain settles on Gateway, the L1 does not have any view of the chain and so the chain trusts the Gateway proof system to ensure that the withdrawal messages it sends to L1 are correct. When the chain migrates back to L1, the ZK Gateway will provide data such as the last batch number of the chain at it settled on top of ZK Gateway. All-in-all, chains should be careful to ensure that they trust the settlement layers they migrate to.

## After settling on top of Gateway

Inside the `L1ChainAssetHandler` we store the `_migrationInterval` mapping, where we store all the past settled layers where the chain has settled to.

> Note, that for the vast majority of chains, it contains only one item as we currently do not support migrating to Gateway multiple times. However, for Era we also preserve the time when it settled on the old Era-based ZK Gateway historically.

The mapping above is used to ensure that the settlement layers that the chain settled to in the past can not attack it in the future in case their ZK system becomes compromised after the chain migrated:

- If a message is claimed to be sent at batch `X` while it was on settlement layer `Y`, it is ensured that the corresponding entry indeed exists there.

Note, that as mentioned above, the "right side" of the range is provided by the settlement layer at the time the chain migrates back to L1, but since it is stored on L1, the malicious chain can not overwrite it.

Additionally, an important note that most chains are even protected against the "completely compromised" settlement layers, since we store the batch roots of each chain (including settlement layers) inside the `L1MessageRoot` too (the `chainBatchRoots` mapping). This mapping was not stored prior to the v31 upgrade however, so the only exception is the Era chain that settled on top of Era-based ZK Gateway and has to trust its implementation to never overwrite the message root. The implementation of this settlement layer is controlled by the decentralized governance, so it can be trusted and it does not contradict the future goal of supporting untrusted settlement layers.

## Chains that never settled on top of Gateway

Due to the `_migrationInterval` mapping mentioned above, chains that are not settling on ZK Gateway should not be affected by malicious settlement layers at all.

## A few final words on `_migrationInterval` mapping

To reiterate, the goal of the mapping is to protect chains against "completely malicious" settlement layers that the chain does not settle to at the moment, while historical withdrawals cannot be overwritten due to the `MessageRoot` restrictions. The only exception is Era and the fact that it settled on top of Era-based ZK Gateway.

> Note, that right now, the bounds of batches that can be used to prove transactions from the chain are queried on L1 during migration to and from Gateway. This means that the chain is protected from a potentially malicious gateway only the moment it finalizes its migration to L1, not the moment the GW batch with the migration transaction settles. More can be read in the comments for the `MigrationInterval` struct. 

However, it does NOT mean that the *codebase as a whole* is ready for "completely malicious" settlement layers (only to "ZK compromised" ones). There are other parts of code that still depend on the correct ZK Gateway L1 implementation.

## Trust assumptions that are required for Gateway itself

The current system requires that if migration on top of ZK Gateway are supported, all the CTMs must be controlled by our decentralized governance. In various places inside the codebase, ZK Gateway relies that neither the chain's implementation nor the CTM's one are malicious. Note, that we are talking only about *Solidity implementation* of Diamond Proxy's facets/ChainTypeManager's implementations only. ZK Gateway must be able to account for potentially compromised ZK systems of the chains that settle on top of it. 

Additionally, before a settlement layer is deployed, it is assumed that the following fields are provided correctly:

- `L1MessageRoot.v31UpgradeChainBatchNumber`. Only a completely compromised chain can return a wrong value here, so the governance should double check that all the chains prior to the transition of the ownership of all the CTMs have this value populated correctly.

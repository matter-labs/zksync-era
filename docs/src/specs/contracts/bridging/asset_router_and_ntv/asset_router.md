# AssetRouters (L1/L2) and NativeTokenVault

<!-- ## Asset router as the main asset bridging entrypoint

The main entry for passing value between chains is the AssetRouter, it is responsible for facilitating bridging between multiple asset types. To read more in detail on how it works, please refer to custom [asset bridging documentation](./asset_router_and_ntv/overview.md).

For the purpose of this document, it is enough to treat the Asset Router as a blackbox that is responsible for processing escrowing funds on the source chain and minting them on the destination chain.

> For those that are aware of the [previous ZKsync architecture](https://github.com/code-423n4/2024-03-zksync/blob/main/docs/Smart%20contract%20Section/L1%20ecosystem%20contracts.md), its role is similar to L1SharedBridge that we had before. Note, however, that it is a different contract with much enhanced functionality. Also, note that the L1SharedBridge will NOT be upgraded to the L1AssetRouter. For more details about migration, please check out [the migration doc](../../upgrade_history/gateway_upgrade/gateway_diff_review.md). -->


The main job of the asset router is to be the central point of coordination for asset bridging. All crosschain token bridging is done between asset routers only and once the message reaches asset router, it then routes it to the corresponding asset handler.

In order to make this easier, all L2 chains have the asset router located on the same address on every chain. It is `0x10003` and it is pre-deployed contract. More on how it is deployed can be seen in the [Chain Genesis](../../chain_management/chain_genesis.md) section.

The endgame is to have L1 asset router have the same functionality as the L2 one. This is not the case yet, but some progress has been made: L2AssetRouter can now bridge L2-native assets to L1, from which it could be bridged to other chains in the ecosystem.

The specifics of the L2AssetRouter is the need to interact with the previously deployed L2SharedBridgeLegacy if it was already present. It has less “rights” than the L1AssetRouter: at the moment it is assumed that all asset deployment trackers are from L1, the only way to register an asset handler on L2 is to make an L1→L2 transaction.

> Note, that today registering new asset deployment trackers will be permissioned, but the plan is to make it permissionless in the future

The specifics of the L1AssetRouter come from the need to be backwards compatible with the old L1SharedBridge. Yes, it will not share the same storage, but it will inherit the need to be backwards compatible with the current SDK. Also, L1AssetRouter needs to facilitate L1-only operations, such as recovering from failed deposits.

Also, L1AssetRouter is the only base token bridge contract that can participate in initiation of cross chain transactions via the bridgehub. This might change in the future.

### L1Nullifier

While the endgoal is to unify L1 and L2 asset routers, in reality, it may not be that easy: while L2 asset routers get called by L1→L2 transactions, L1 ones don't and require manual finalization of transactions, which involves proof verification, etc. To move this logic outside of the L1AssetRouter, it was moved into a separate L1Nullifier contract.

_This is the contract the previous L1SharedBridge will be upgraded to, so it should have the backwards compatible storage._

> New bridge contracts

### A separate L2Nullifier does not exist

The L1Nullifier stores two things: 
1. finalized L2->L1 withdrawals
1. initiated L1->L2 priority transactions

1. is needed on L1 as L2->L1 txs are executed arbitrarily, so we need to record whether they happened or not to stop double spending. However, L1->L2 priority txs are enforced to be executed only once, so we don't need a separate L2Nullifier. Finally, L2->L2 txs are executed in the InteropHandler, which does store if they were executed or not (so the L2Nullifier is built into the InteropHandler).

2. The initiated L2->L1 and L2->L2 transactions are not stored. This is needed for L1->L2 txs, as priority transactions have to be executed by the system, and cannot be retried if they fail. So failed deposits have to be redeemable on L1. L2->L2 and L2->L1 txs might also fail, but if they fail due to gas reasons, they can be retried. If they fail due to contract error, then the receiving contract has to be fixed ( another way of explaining it, there can always be a contract error even for claiming failed deposits. So the only sustainable way of fixing contract errors is to fix the contract).

For this reason, on the L2 claiming failed deposits and bridgehubConfirmL2Transaction are not implemented.
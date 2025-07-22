# AssetRouter

> **Overview:**  
> - **On L1:** The **L1 Asset Router** handles cross‑chain asset coordination _and_ is complemented by the **L1 Nullifier** for additional finalization and recovery features.
> - **On L2:** Only the **L2 Asset Router** is pre-deployed at the same address (`0x10003`) on every L2 chain.  

### AssetRouter (L1/L2)

The main job of the asset router is to be the central point of coordination for asset bridging. All crosschain token bridging is done between asset routers only and once the message reaches asset router, it then routes it to the corresponding asset handler.

In order to make this easier, all L2 chains have the asset router located on the same address on every chain. It is `0x10003` and it is pre-deployed contract. More on how it is deployed can be seen in the [Chain Genesis](../../chain_management/chain_genesis.md) section.

The endgame is to have L1 asset router have the same functionality as the L2 one. This is not the case yet, but some progress has been made: L2AssetRouter can now bridge L2-native assets to L1, from which it could be bridged to other chains in the ecosystem.

Examples of differences in functionality are:

1. Failed‐deposit recovery.
   The `L1AssetRouter` provides on-chain recovery for failed L1→L2 deposits via its `bridgeRecoverFailedTransfer` and `claimFailedDeposit` functions. The `L2AssetRouter` has no equivalent, because L2→L1 withdrawals are atomic on L2: burning the L2 token and sending the L2→L1 withdrawal message occur in the same transaction. If anything goes wrong, the entire transaction reverts and no token is burned and no message is sent.
2. User-initiated L1→L2 deposits must go through Bridgehub (or the legacy bridge), not directly.
   There is no public `deposit(...)` on `L1AssetRouter` you can call as a user—only, by contrast, on L2 you have a public `withdraw(...)` method. 
   In case of `L1AssetRouter` user has to start deposit through either `Bridgehub` or `L1ERC20Bridge` (legacy bridge). 
   However in case of `L2AssetRouter` user is able to simply call `withdraw` function in `L2AssetRouter`.

The specifics of the L2AssetRouter is the need to interact with the previously deployed L2SharedBridgeLegacy if it was already present. It has less “rights” than the L1AssetRouter: at the moment it is assumed that all asset deployment trackers are from L1, the only way to register an asset handler on L2 is to make an L1→L2 transaction.

> Note, that today registering new asset deployment trackers is permissioned, but the plan is to make it permissionless in the future

The specifics of the L1AssetRouter come from the need to be backwards compatible with the old L1SharedBridge. Yes, it does not share the same storage, but it inherits the need to be backwards compatible with the current SDK. Also, L1AssetRouter needs to facilitate L1-only operations, such as recovering from failed deposits.

Also, L1AssetRouter is the only base token bridge contract that can participate in initiation of cross chain transactions via the bridgehub. This might change in the future.

### L1Nullifier

While the endgoal is to unify L1 and L2 asset routers, in reality, it may not be that easy: while L2 asset routers get called by L1→L2 transactions, L1 ones don't and require manual finalization of transactions, which involves proof verification, etc. To move this logic outside of the L1AssetRouter, it was moved into a separate L1Nullifier contract.

_This is the contract the previous L1SharedBridge had been upgraded to, so it has the backwards compatible storage._

### A separate L2Nullifier does not exist

The L1Nullifier stores two things: 
1. finalized L2->L1 withdrawals
1. initiated L1->L2 priority transactions

1. is needed on L1 as L2->L1 txs are executed arbitrarily, so we need to record whether they happened or not to stop double spending. However, L1->L2 priority txs are enforced to be executed only once, so we don't need a separate L2Nullifier.  Finally, L2->L2 txs are executed in the InteropHandler, which does store if they were executed or not (so the L2Nullifier is built into the InteropHandler).

2. The initiated L2->L1 and L2->L2 transactions are not stored. This is needed for L1->L2 txs, as priority transactions have to be executed by the system, and cannot be retried if they fail. So failed deposits have to be redeemable on L1. L2->L2 and L2->L1 txs might also fail, but if they fail due to gas reasons, they can be retried. If they fail due to contract error, then the receiving contract has to be fixed ( another way of explaining it, there can always be a contract error even for claiming failed deposits. So the only sustainable way of fixing contract errors is to fix the contract).

For this reason, on the L2 claiming failed deposits and bridgehubConfirmL2Transaction are not implemented.

### L2SharedBridgeLegacy

L2AssetRouter is pre-deployed onto a specific address. The old L2SharedBridge is upgraded to L2SharedBridgeLegacy contract. The main purpose of this contract is to ensure compatibility with the incoming deposits and re-route them to the shared bridge.

This contract is never deployed for new chains.

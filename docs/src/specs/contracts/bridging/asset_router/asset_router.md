# AssetRouters (L1/L2) and NativeTokenVault

[back to readme](../../README.md)

The main job of the asset router is to be the central point of coordination for bridging. All crosschain token bridging is done between asset routers only and once the message reaches asset router, it then routes it to the corresponding asset handler.

In order to make this easier, all L2 chains have the asset router located on the same address on every chain. It is `0x10003` and it is pre-deployed contract. More on how it is deployed can be seen in the [Chain Genesis](../../chain_management/chain_genesis.md) section.

The endgame is to have L1 asset router have the same functionality as the L2 one. This is not the case yet, but some progress has been made: L2AssetRouter can now bridge L2-native assets to L1, from which it could be bridged to other chains in the ecosystem.

The specifics of the L2AssetRouter is the need to interact with the previously deployed L2SharedBridgeLegacy if it was already present. It has less “rights” than the L1AssetRouter: at the moment it is assumed that all asset deployment trackers are from L1, the only way to register an asset handler on L2 is to make an L1→L2 transaction.

> Note, that today registering new asset deployment trackers will be permissioned, but the plan is to make it permissionless in the future

The specifics of the L1AssetRouter come from the need to be backwards compatible with the old L1SharedBridge. Yes, it will not share the same storage, but it will inherit the need to be backwards compatible with the current SDK. Also, L1AssetRouter needs to facilitate L1-only operations, such as recovering from failed deposits.

Also, L1AssetRouter is the only base token bridge contract that can participate in initiation of cross chain transactions via the bridgehub. This will change in the future with the support of interop.

### L1Nullifier

While the endgoal is to unify L1 and L2 asset routers, in reality, it may not be that easy: while L2 asset routers get called by L1→L2 transactions, L1 ones don't and require manual finalization of transactions, which involves proof verification, etc. To move this logic outside of the L1AssetRouter, it was moved into a separate L1Nullifier contract.

_This is the contract the previous L1SharedBridge will be upgraded to, so it should have the backwards compatible storage._

### NativeTokenVault (L1/L2)

NativeTokenVault is an asset handler that is available on all chains and is also predeployed. It is provides the functionality of the most basic bridging: locking funds on one chain and minting the bridged equivalent on the other one. On L2 chains NTV is predeployed at the `0x10004` address.

The L1 and L2 versions of the NTV are almost identical in functionality, the main differences come from the differences of the deployment functionality in L1 and L2 envs, where the former uses standard CREATE2 and the latter uses low level calls to `CONTRACT_DEPLOYER`system contract.

Also, the L1NTV has the following specifics:

- It operates the `chainBalance` mapping, ensuring that the chains do not go beyond their balances.
- It allows recovering from failed L1→L2 transfers.
- It needs to both be able to retrieve funds from the former L1SharedBridge (now this contract has L1Nullifier in its place), but also needs to support the old SDK that gives out allowance to the “l1 shared bridge” value returned from the API, i.e. in our case this is will the L1AssetRouter.

### L2SharedBridgeLegacy

L2AssetRouter has to be pre-deployed onto a specific address. The old L2SharedBridge will be upgraded to L2SharedBridgeLegacy contract. The main purpose of this contract is to ensure compatibility with the incoming deposits and re-route them to the shared bridge.

This contract is never deployed for new chains.

### Summary

![image.png](./img/bridge_contracts.png)

> New bridge contracts

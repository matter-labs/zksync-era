# Native Token Vault
[back to readme](../../README.md)

### NativeTokenVault (L1/L2)

NativeTokenVault is an asset handler that is available on all chains and is also predeployed. It is provides the functionality of the most basic bridging: locking funds on one chain and minting the bridged equivalent on the other one. On L2 chains NTV is predeployed at the `0x10004` address.

The L1 and L2 versions of the NTV are almost identical in functionality, the main differences come from the differences of the deployment functionality in L1 and L2 envs, where the former uses standard CREATE2 and the latter uses low level calls to `CONTRACT_DEPLOYER`system contract.

Also, the L1NTV has the following specifics:

<!-- - It operates the `chainBalance` mapping, ensuring that the chains do not go beyond their balances.  -->
- It allows recovering from failed L1→L2 transfers.
- It needs to both be able to retrieve funds from the former L1SharedBridge (now this contract has L1Nullifier in its place), but also needs to support the old SDK that gives out allowance to the “l1 shared bridge” value returned from the API, i.e. in our case this is will the L1AssetRouter.

### L2SharedBridgeLegacy

L2AssetRouter has to be pre-deployed onto a specific address. The old L2SharedBridge will be upgraded to L2SharedBridgeLegacy contract. The main purpose of this contract is to ensure compatibility with the incoming deposits and re-route them to the shared bridge.

This contract is never deployed for new chains.

### Summary

![image.png](./img/bridge_contracts.png)
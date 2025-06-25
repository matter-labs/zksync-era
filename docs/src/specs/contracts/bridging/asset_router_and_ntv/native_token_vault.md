# Native Token Vault

### NativeTokenVault (L1/L2)

NativeTokenVault is an asset handler that is available on all chains and is also predeployed. It provides the functionality of the most basic bridging: locking funds on one chain and minting the bridged equivalent on the other one. On L2 chains NTV is predeployed at the `0x10004` address. NativeTokenVault acts as the default AssetHandler, so regular ERC-20 tokens can use it unless custom bridging logic or special features are required.

The L1 and L2 versions of the NativeTokenVault share the same core functionality, but differ in their deployment mechanics and certain L1-specific responsibilities.

On L1, the contract is deployed using standard CREATE2, while on L2 it uses low-level calls to the CONTRACT_DEPLOYER system contract.

Also, the L1NTV has the following specifics:

- It operates the `chainBalance` mapping, ensuring that the chains do not go beyond their balances.
- It allows recovering from failed L1→L2 transfers.
- It needs to both be able to retrieve funds from the former L1SharedBridge (now this contract has L1Nullifier in its place), but also needs to support the old SDK that gives out allowance to the “l1 shared bridge” value returned from the API, i.e. in our case this is will the L1AssetRouter.

### Summary

![image.png](./img/bridge_contracts.png)

> New bridge contracts
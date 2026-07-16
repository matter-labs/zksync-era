<!--- WIP --->

# Bridgehub

## Introduction

Bridgehub is the main chain registry contract for the ecosystem, that stores:

- A mapping from chainId to chains address
- A mapping from chainId to the CTM it belongs to.
- A mapping from chainId to its base token (i.e. the token that is used for paying fees)
- Whitelisted settlement layers (i.e. Gateway)

Note sure what CTM is? Check our the [overview](./chain_type_manager.md).

> This document will not cover how ZK Gateway works, you can check it out in [a separate doc](../gateway/overview.md). 

The Bridgehub is the contract where new chains can [register](./chain_genesis.md). Chain migration between settlement layers is handled by dedicated `ChainAssetHandler` contracts (`L1ChainAssetHandler` on L1, `L2ChainAssetHandler` as an L2 predeploy at `0x1000a`), not by the Bridgehub directly. The Bridgehub retains only helper hooks gated `onlyChainAssetHandler` (`forwardedBridgeBurnSetSettlementLayer`, `forwardedBridgeMint`, `forwardedBridgeConfirmTransferResult`, and `registerNewZKChain`). Read more about chain migration [here](../gateway/chain_migration.md).

Overall, it is the main registry for all the contracts. Note, that a clone of Bridgehub is also deployed on each L2 chain, it is used to start interop txs by checking that the chain is active. It is also used on settlement layers such as Gateway. All the in all, the architecture of the entire ecosystem can be seen below:

![Contracts](./img/ecosystem_architecture.png)
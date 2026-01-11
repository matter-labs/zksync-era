# Overview of Custom Asset Bridging with the Asset Router

Bridges are completely separate contracts from the ZKChains and system contracts. They are a wrapper for L1 <-> L2 communication on both L1 and L2. Upon locking assets on one layer, a request is sent to mint these bridged assets on the other layer.
Upon burning assets on one layer, a request is sent to unlock them on the other.

Custom asset bridging is a new bridging model that allows to:

1. Minimize the effort needed by custom tokens to be able to become part of the elastic chain ecosystem. Before, each custom token would have to build its own bridge, but now just custom asset deployment trackers / asset handler is needed. This is achieved by building a modular bridge which separates the logic of L1<>L2 messaging from the holding of the asset.
2. Unify the interfaces between L1 and L2 bridge contracts, paving the way for easy cross chain bridging. It will especially become valuable once interop is enabled.

#### New concepts

- assetId => identifier to track bridged assets across chains. This is used to link messages to specific asset handlers in the AssetRouters. Note, that since the same asset could be created on multiple chains, assetId is not just an address. It's obtained by combining chain id, NTV as asset deployment tracker and asset data. Usually, address is used as asset data field. This was assets that share the same address on different chains will have different assetId.
- AssetHandler => contract that manages liquidity (burns/mints, locks/unlocks, etc.) for specific token (or a set of them) on a chain.
- AssetDeploymentTracker => contract that manages the deployment of asset handlers across chains. This is the contract that registers these asset handlers in the AssetRouters.
- AssetRouter => contract that coordinates crosschain token bridging and routes messages to the correct AssetHandler.

### Normal flow

Assets Handlers are registered in the Routers based on their assetId. The assetId is used to identify the asset when bridging, it is sent with the cross-chain transaction data and Router routes the data to the appropriate Handler. If the asset handler is not registered in the L2 Router, then the L1->L2 bridging transaction will fail on the L2 (expect for NTV assets, see below).

Asset registration is handled by the AssetDeploymentTracker. It is expected that this contract is deployed on the L1. Registration of the assetHandler on a ZKChain can be permissionless depending on the Asset (e.g. the AssetHandler can be deployed on the chain at a predefined address, this can message the L1 ADT, which can then register the asset in the Router). Registering the L1 Handler in the L1 Router can be done via a direct function call from the L1 Deployment Tracker. Registration in the L2 Router is done indirectly via the L1 Router.

#### Registering a custom asset

`assetId = keccak256(chainId, asset deployment tracker = msg.sender, additionalData)`

- AssetHandler Registration for New Tokens
  Before you can bridge a new token, you must register its AssetHandler in both routers. However, for NTV (Native Token Vault) assets this step isn’t required. If an L1→L2 transaction is submitted without a registered AssetHandler, it won’t fail—instead, the message is automatically forwarded to the L2NTV contract. The L2NTV then verifies that the asset was deployed by the L1NTV by checking that the `assetId` embeds the correct ADT address (for NTV assets, the ADT is the NTV and the relevant address is the L2NTV address). If the `assetId` is valid, the token contract is deployed on L2.

- Automatic Bridging via the Native Token Vault
  The Native Token Vault is a special AssetHandler designed for automatic bridging. It lets you bridge an L1 token to L2 without pre-deploying the token contract or registering it in the L2 router. Under the hood, unregistered AssetHandler messages are forwarded to the L2NTV, which performs the same `assetId` check and, upon validation, deploys the corresponding token contract.

- Custom Asset Registration Flow  
   1. Deploy the Asset Handlers (on L1 and L2), the custom token contracts, and the Asset Deployment Tracker (ADT). The ADT is expected to be deployed on L1.  
   2. The ADT calls `setAssetHandlerAddressThisChain` on `L1AssetRouter` to register itself and its corresponding Asset Handler by updating the appropriate mapping.  
   3. During an L1→L2 deposit, the internal method `_setAssetHandlerAddressOnCounterpart` is invoked automatically to propagate the registration to the L2 router.  

   A visualization of this process can be seen here:  
   [Asset Registration](./img/custom_asset_handler_registration.png)

#### Bridging an already-registered asset

- From L1 to L2  
   1. The user calls `requestL2TransactionTwoBridges` on `Bridgehub`, specifying `L1AssetRouter` as the second bridge address, along with all other parameters required to initiate the transfer.  
   2. Under the hood, `Bridgehub`, `L1AssetRouter`, and the Asset Handler contracts verify the validity of the provided data, value, and allowances. If everything checks out, the `bridgehubRequestL2Transaction` method is called on the ZK Chain, appending a priority transaction to `L2AssetRouter.finalizeDeposit`.  
   3. The operator picks up the priority transaction. If it executes successfully, the transfer completes. Otherwise, the user can reclaim their assets via the `claimFailedDeposit` function in `L1AssetRouter`, providing proof that the transaction indeed failed on L2.

   A visualization of steps 1–2 can be seen here:  
   [RequestL2TransactionTwoBridges for token deposit](./img/requestl2transactiontwobridges_token_deposit.png).

- From L2 to L1  
   1. The user calls `L2AssetRouter.withdraw(assetId, assetData)` directly. Here, `assetData` varies by Asset Handler; it’s parsed by the handler into the format it requires.  
   2. The L2 handler burns the tokens, and `L2AssetRouter` sends an L2→L1 message.  
   3. Once the L2→L1 message arrives on L1, the user calls `finalizeWithdrawal` on `L1AssetRouter`, providing the chain ID where the withdrawal was initiated, the message’s position in that chain, and proof of its inclusion. The `L1Nullifier` contract verifies the proof, and if it’s valid, control returns to `L1AssetRouter`, which then calls the appropriate Asset Handler (based on `assetId`) to release the assets.

### Read more

You can read more in the more in-depth about L1 and L2 asset routers and the default asset handler that is Native Token Vault [here](./asset_router.md).

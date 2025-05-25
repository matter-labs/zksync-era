# Glossary

- **Validator/Operator** - a privileged address that can commit/verify/execute L2 batches.
- **L2 batch (or just batch)** - An aggregation of multiple L2 blocks. Note, that while the API operates on L2 blocks,
  the prove system operates on batches, which represent a single proved VM execution, which typically contains multiple
  L2 blocks.
- **Facet** - implementation contract. The word comes from the EIP-2535.
- **Gas** - a unit that measures the amount of computational effort required to execute specific operations on the
  ZKsync Era network.
- **MessageRoot**, **ChainRoot**, **ChainBatchRoot**, **LocalLogsRoot** , **L2ToL1LogsRoot**- different nodes in the recursive Merkle tree used to aggregate messages. Note, LocalLogsRoot and L2ToL1LogsRoot are the same. 
- **assetId** - unique 32 bytes used to identify different assets in the AssetRouter.
- **Settlement Layer** - the layer where a chains settles its batches. Can be L1 or Gateway.

Some terms are inherited from the wider [Ethereum ecosystem](https://github.com/ethereum/L2-interop/blob/main/NOMENCLATURE.md). 

List of contracts and abbreviations:
- Chain Manager Contracts
  - Bridgehub

    Main ecosystem contract. Tracks all the chains and their types (i.e. CTMs), based on their chain ID and addresses. This is where new chains register. 

  - ChainTypeManager (CTM) Contract:
  
    used to coordinate upgrades for a certain chain classes. We only support a single CTM currently. 
  
  - CTMDeploymentTracker: 

    Tracks the deployment of a new CTM on different settlement layers, such as Gateway. Needed as the Bridgehub is registered in the AssetRouter as an AssetHandler for chains so that they can be migrated to the Gateway.

- Chain contracts
  - Diamond Proxy: A type of proxy contract (i.e. like Transparent Upgradable, Universal Upgradable). Currently only the ZK chains use this contract, so it is used sometimes as a synonym for the chain.
    - MailboxFacet: functions on the chain used to send and receive messages to the L1
    - GetterFacet: functions that are read-only and can be called by anyone
    - AdminFacet: functions that are used to manage the chain
    - ExecutorFacet: functions that are used to execute L2 batches
    - Verifier
    - Additional contracts:
      - ValidatorTimelock
      - DiamondInit
      - PriorityQueue
      - PriorityTree
      - MessageVerification
      - Any upgrade contract, i.e. GatewayUpgrade

- Messaging related contracts:
  - MessageRoot
  - L1Nullifier

- Asset related contracts:
  - AssetRouter
  - NativeTokenVault
  - Additional contracts:
    - BridgeStandardERC29
    - IAssetHandler
    - DeploymentTracker
  - Legacy: 
    - L1Erc20Bridge
    - L2SharedBridge (upgraded from L2Erc20Bridge)

- DA contracts: 
  - CalldataDA
  - CaddlataDAGateway
  - RelayedSLDAValidator
  - RollupDAManager
  - ValidiumL1DAValidator

- Libraries and Primitives: 
  - DynamicIncrementalMerkleTree
  - FullMerkleTree
  - MessageHashing
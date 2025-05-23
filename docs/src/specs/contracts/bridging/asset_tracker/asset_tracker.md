# Contract-based and Full AssetTracker, ZK IP, Firewall

## Introduction

The AssetTracker is a component that provides additional security, by tracking the balance of chains on the chains settlement layer (Gateway and L1). This is done by parsing all interop txs on the SL and updating the balance of the chains.

### Name ideas

Firewall, ZK IP, AssetTracker. TBD.

## Bulkheads

Today, we have bulkheads inside NTV. 

- currently we store the balance of the token only on L1
- when chain deposits, withdraws from L1, we increase decrease the balance.
- For L2 Native tokens, we track the origin chainId. We don’t track the balance of that chain, since the token can be minted arbitrarily. We store the origin

```solidity
mapping(chainId => assetId => balance) chainBalance;
mapping(bytes32 assetId => uint256 originChainId) public originChainId;
```

### Contract based ZK IP  (internet protocol) / AssetTracker

ZK IP should :

- enable interop
    - Bulkheads do not work with interop, since interop changes the balance of a chain without touching L1.
- be compatible with any assetId.

To differentiate chains that can mint the asset, we will have minter roles. For these the balance of the chain will not be enforced. 

```solidity
mapping(chainId => assetId => bool) isMinterChain;
```

To enable interop, besides updating based on L1<>L2 txs, we will have to [parse all interop txs](https://github.com/matter-labs/era-contracts/blob/b5fda9c4dd8171ffb53337711fe8da43b4266026/l1-contracts/contracts/bridge/asset-tracker/AssetTrackerBase.sol#L35). This means we have to parse all L2toL1 logs, if the sender is the interopCenter parse the message, and update the balance on the SL. 

We can have non balance changing operations. This is useful to set the `isMinterChain` role

When settling each batch, each chain would call the following function: 

```solidity
// Called only when the batch is executed

contract AssetTracker {
	function parseLogsAndMessages(
			L2Log[] calldata _logs, 
			bytes[] calldata _messages, 
			bytes32 messageRoot) 
		external returns (Ops[] memory operations) {
			operations = // parse logs and messages to get ZK IP ops
			// make sure that AggregateRoot = keccak(localRoot, messageRoot) is the same as in DiamondProxy
			processOps(operations)
		}
	
	function processOps(Ops[] memory operations) external {
	   uint256 chain = chainidFromAddress[msg.sender];
	   
	   for (op : operations) {
	      if (op.type == AddBalance) {
	         if (!minter[chain][op.assetId] && !adt_preimage(op.assetId, chain, op.additionalInfo)) {
	            // checks for overflow and reduces balance
	            balance[chain][op.assetId] -= op.amount;
	         }
	         balance[op.chainTo][op.assetId] += op.amount;
	      } else if(op.type == AddMinter) {
	         // check that the ADT belongs to the chain
	         if (adt_preimage(op.assetId, chain, op.additionalInfo) {
			        isMinterChain[chain][op.assetId] = true;
	         }
	      }
	   }
	}
	
	// function that checks whether this chainId is part of the assetid preimage.
	function adt_preimage(uint256 chainId, ...) {
	
	}
}
```

### Migrating and settling on Gateway

When a chain migrates from L1 to GW or from GW to L1, the `chainBalance` mapping does not get transferred automatically:

- They can be trustlessly moved by anyone from GW to L1 and vice versa:

```bash
function moveBalanceToSL(uint256 chainId, bytes32 assetId) {
	address sl = GettersFacet(chainToAddress(chainId)).getSettlementLayer();
	
	// cant move anywhere from settlement layer
	require(sl != block.chainid);
	
	uint256 currentBalance = balance[chainId][assetId];
	balance[chainId][assetId] = 0;
	balance[settlmentLayer][assetId] += currentBalance;
	
	sendTxToSL(chinId, assetId, currentBalance);
}
```

 - The same goes for the minter role.

The AssetTracker contracts will be deployed on the GW and the L1, but we might only allow interop and asset trackers on GW, since using them on L1 is expensive. 

On GW we will also track the balances of the chains. To do this we will parse the L1->L3 messages as they are processed, and L3->L1 messages are processed same as L2->L1 messages on L1. 

## How full ZK IP could look like with the same user interface (+ migration) could look like

Processing and updating all the logs on L1 or GW does not scale. Full ZK IP would be a zk validium running in parallel to the main chain, storing the balance of the chain and processing the logs. Instead of passing in all logs on the SL, we would only pass in a zk proof.  

For each chain we would still have the mappings, ( without the chainId, since it applies for the current chain):

```solidity
mapping(assetId => balance) chainBalance;
mapping(assetId => bool) isMinterChain;
```

The main difference is that we cannot update the destination chain’s balance when sending the messages in `parseLogsAndMessages` since now the chainBalance mappings for different chains are on different chains. This means we will have to import the receiving/incoming messages. We do this the same way we do interop: via the MessageRoot, merkle proofs, and L2Nullifier to not double mint. 

```solidity
/// similar to the one on the L2
contract L2MessageRootStorageForAssetTracking {
	mapping(uint256 chainId => mapping(uint256 batchNumber => bytes32 msgRoot)) public msgRoots;
}

contract L2NullifierForAssetTracking {
	mapping(txHash => bool isConsumed)
}

contract InteropHandlerForAssetTracking {
	function executeBundle(InteropBundle) {
		...
	}
}
```

With each state transition:

- Chain provides a the root of the global tree `MessageRoot` that it used to apply add operations from.
- Root hash of the zk ip chain, this includes:
    - The new state of the nullifier
    - the state of the imported messageRoots ( these should be preserved )
    - new state of the balances, isMinterRole
- The chains exported messages FullMessageRoot
    - in FullMessageRoot = keccak(localRoot, AggregatedRoot)
    

### Withdrawals to L1/other chains

Withdrawals to L1 are just ZK IP messages that mint funds for L1. This is the same as for other L2s.

When a user wants to withdraw funds, it needs to provide a proof for the corresponding ZK IP message. 

  

### Migration to full ZK IP

When we are ready to move to the full ZK IP, we could add a operation available to everyone called: “consume bulkhead”, it would append an `add` operation to the `MessageRoot`. The chain can then consume this operation into its local tree later on.
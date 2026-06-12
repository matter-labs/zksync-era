# Priority Operations: Merkle Tree

## Overview

The priority tree is the live mechanism for handling L1→L2 priority operations. `Mailbox._writePriorityOpHash` appends each new priority op into `s.priorityTree` (an incremental Merkle tree), and batch execution verifies priority ops via Merkle proofs (`s.priorityTree.processBatch`) over `PriorityOpsBatchInfo` against historical roots.

## Motivation for migration to Merkle Tree

Gateway was introduced, requiring support for one more operation:

- migrating priority queue from L1 to Gateway (and back)

Current implementation takes `O(n)` space and is vulnerable to spam attacks during migration
(e.g. an attacker can insert a lot of priority operations and migration of all of them would have been impossible due to gas limits).

Hence, we required an implementation with a small (constant- or log-size) space imprint that could be migrated to Gateway and back that would still allow us to perform the other 2 operations.

Merkle tree of priority operations is perfect for this since the latest root hash can simply be migrated to Gateway and back.

- It can still efficiently (in `O(height)`) insert new operations.
- It can also still efficiently (in `O(n)` compute and `O(n + height)` calldata) check that the batch’s `priorityOperationsHash` corresponds to the operations from the queue.

Note that `n` here is the number of priority operations in the batch, not `2^height`.

The implementation details are described below.

### FAQ

- Q: Why can't we just migrate the rolling hash of the operations in the existing priority queue?
- A: The rolling hash is not enough to check that the operations from the executed batch are indeed from the priority queue. That would have required storing all historical rolling hashes, which would have been `O(n)` space and would not have solve the spam attack problem.

## Implementation

The implementation consists of two parts:

- Merkle tree on L1 contracts, to replace the existing priority queue (while still supporting the existing operations)
- Merkle tree off-chain on the server, to generate the merkle proofs for the executed priority operations.

### Contracts

On the contracts, the Merkle tree is implemented as an Incremental (append-only) Merkle Tree ([example implementation](https://github.com/tornadocash/tornado-core/blob/master/contracts/MerkleTreeWithHistory.sol)), meaning that it can efficiently (in `O(height)` compute) append new elements to the right, while only storing `O(height)` nodes at all times.

It is also dynamically sized, meaning that it doubles in size when the current size is not enough to store the new element.

### Server

On the server, the Merkle tree is implemented as an extension of `MiniMerkleTree` currently used for L2->L1 logs.

It has the following properties:

- in-memory: the tree is stored in memory and is rebuilt on each restart (details below).
- dynamically sized (to match the contracts implementation)
- append-only (to match the contracts implementation)

The tree does not need to be super efficient, since we process on average 7 operations per batch.

### Why in-memory?

Having the tree in-memory means rebuilding the tree on each restart. This is fine because on mainnet after >1 year since release we have only 3.2M priority operations. We only have to fully rebuild the tree _once_ and then simply cache the already executed operations (which are the majority). Having the tree in-memory has an added benefit of not having to have additional infrastructure to store it on disk and not having to be bothered to rollback its state manually if we ever have to (as we do for e.g. for the storage logs tree).

Note: If even rebuilding it once becomes a problem, it can be easily mitigated by only persisting the cache nodes.

### Caching

**Why do we need caching?** After a batch is successfully executed, we no longer need to have the ability to generate merkle paths for those operations. This means that we can save space and compute by only fully storing the operations that are not yet executed, and caching the leaves
corresponding to the already executed operations.

We only cache some prefix of the tree, meaning nodes in the interval [0; N) where N is the number of executed priority operations. The cache stores the rightmost cached left-child node on each level of the tree (see diagrams).

![Untitled](./img/PQ1.png)

![Untitled](./img/PQ2.png)

![Untitled](./img/PQ3.png)

This means that we are not able to generate merkle proofs for the cached nodes (and since they are already executed, we don't need to). This structure allows us to save a lot of space, since it only takes up `O(height)` space instead of linear space for all executed operations. This is a big optimization since there are currently 3.2M total operations but <10 non-executed operations in the mainnet priority queue, which means most of the tree is cached.

This also means we don’t really have to store non-leaf nodes other than cache, since we can calculate merkle root / merkle paths in `O(n)` where `n` is the number of non-executed operations (and not total number of operations), and since `n` is so small, it is really fast.

### Adding new operations

On the contracts, appending a new operation to the tree is done by simply calling `append` on the Incremental Merkle Tree, which updates at most `height` slots. Actually, it works almost exactly like the cache described above. Once again: [tornado-cash implementation](https://github.com/tornadocash/tornado-core/blob/1ef6a263ac6a0e476d063fcb269a9df65a1bd56a/contracts/MerkleTreeWithHistory.sol#L68).

On the server, `eth_watch` listens for `NewPriorityRequest` events as it does now, and appends the new operation to the tree on the server.

### Checking validity

To check that the executed batch indeed took its priority operations from the queue, we have to make sure that if we take first `numberOfL1Txs` non-executed operations from the tree, their rolling hash matches `priorityOperationsHash` . Since we don't store the hashes of these operations onchain anymore, we provide them as calldata. Additionally in calldata, we should provide merkle proofs for the **first and last** operations in that batch (hence `O(n + height)` calldata). This makes it possible to prove onchain that that contiguous interval of hashes indeed exists in the merkle tree.

This can be done simply by constructing the part of the tree above this interval using the provided paths to first and last elements of the interval checking that computed merkle root matches with stored one (in `O(n)` where `n` is number of priority operations in a batch). We also need to track the `index` of the first unexecuted operation onchain to properly calculate the merkle root and ensure that batches don’t execute some operations out of order or multiple times.

We also need to prove that the rolling hash of provided hashes matches with `priorityOperationsHash` which is also `O(n)`

It is important to note that we store some number of historical root hashes, since the Merkle tree on the server might lag behind the contracts a bit, and hence merkle paths generated on the server-side might become invalid if we compare them to the latest root hash on the contracts. These historical root hashes are not necessary to migrate to and from Gateway though.


## Legacy: Priority Queue

Previously, priority operations were stored in a linear queue (PriorityQueue library) using `pushBack`/`popFront`, with batch execution verifying ops via a rolling hash loop. This was replaced by the Merkle tree to support Gateway migration. The old library remains in storage as `s.__DEPRECATED_priorityQueue` but is no longer in the request/execute path.
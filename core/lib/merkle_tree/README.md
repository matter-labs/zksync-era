# Merkle Tree

This cargo contains the basic functions to create and modify the Merkle Tree.

We're using a classic binary tree here (not Trie, not B-trees etc) to make it easier for the circuit creation. Also the
depth of the tree is fixed to 256.

At any given moment, the storage keeps the tree only at a given block (and that block number is encoded in
`block_number` row) - it can be accessed via `ZkSyncTree` stuct.

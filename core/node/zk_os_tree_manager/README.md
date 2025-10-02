# ZK OS Tree Manager

Node component responsible for managing the ZK OS Merkle tree. Functionally equivalent to the metadata calculator for
the Era tree, except that it does not participate in witness generation (i.e., only supports the lightweight tree mode
in the metadata calculator terms).

## Scope of responsibilities

- Updating the tree based on newly recorded blocks.
- Providing tree REST API.
- _(Not implemented yet)_ Tree recovery from a snapshot.
- _(Not implemented yet)_ Tree pruning.

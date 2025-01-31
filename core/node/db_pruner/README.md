# `zksync_node_db_pruner`

Database pruner is a component that regularly removes the oldest l1 batches from the database together with
corresponding L2 blocks, events, etc.

There are two types of objects that are not fully cleaned:

- **Transactions** only have `BYTEA` fields cleaned as some components rely on transactions existence.
- **Storage logs:** only storage logs that have been overwritten are removed

## Pruning workflow

_(See [node docs](../../../docs/src/guides/external-node/08_pruning.md) for a high-level pruning overview)_

There are two phases of pruning an L1 batch, soft pruning and hard pruning. Every batch that would have its records
removed if first _soft-pruned_. Soft-pruned batches cannot safely be used. One minute (this is configurable) after soft
pruning, _hard pruning_ is performed, where hard means physically removing data from the database.

The reasoning behind this split is to allow node components such as the API server to become aware of planned data
pruning, and restrict access to the pruned data in advance. This ensures that data does not unexpectedly (from the
component perspective) disappear from Postgres in a middle of an operation (like serving a Web3 request). At least in
some case, like in VM-related Web3 methods, we cannot rely on database transactions for this purpose.

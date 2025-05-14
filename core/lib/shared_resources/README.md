# Shared Resources for ZKsync Node Framework

This library contains resources shared among multiple components in the ZKsync node.

## Developer notes

This library should contain the following resources:

- Resources that don't have a clear component crate to place into (e.g., contract data resources).
- Resources that may have a component crate, but placing them there would introduce unreasonable tight coupling between
  component crates. As an example, the node sync state _could_ be placed in the
  [`zksync_node_sync`](../../node/node_sync) crate, but it would mean that
  [the API server component](../../node/api_server) would need to rely on it, and there would be cyclical dependency
  between `zksync_node_sync` and [the state keeper](../../node/state_keeper).

Resources in the library should not bring heavyweight dependencies (roughly speaking, anything not already brought by
the [node framework](../node_framework)).

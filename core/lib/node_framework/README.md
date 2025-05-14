# ZKsync Node DI Framework

This is a dependency injection (DI) library used by the ZKsync node binaries ([`zksync_server`] and
[`zksync_external_node`]). The library is designed to be fully generic, so it doesn't incorporate any ZKsync-specific
domain knowledge.

## Usage

The library provides following DI abstractions:

- **Resources** are provided or requested by node components. An example of a resource is a Postgres connection pool
  factory.
- Components may also provide **tasks** managed by the framework.
- Requesting and providing resources / tasks is encapsulated in **wiring layers**.

Wiring layers are defined in a decentralized way in [component crates](../../node) and some [lower-level libraries](..)
and are assembled together in the node binary using the **service builder**.

### How to work with DI

Define DI logic in the separate `node` module of your crate.

For lower-level libraries, the module should be gated behind an opt-in `node_framework` feature (i.e., via
`#[cfg(feature = "node_framework")]`). Correspondingly, the node framework should be an optional dependency, together
with any auxiliary node-specific dependencies:

```toml
[dependencies]
zksync_node_framework = { workspace = true, optional = true }

[features]
default = []
node_framework = ["dep:zksync_node_framework"]
```

The node framework should not require many additional deps, _maybe_ [health checks](../health_check) and/or
[shared DI resources](../shared_di); if it requires heavyweight deps, you're probably doing something wrong. The
reasoning behind feature-gating is that it's reasonable to assume that lower-level libraries may be used outside node
binaries.

For node components, no feature-gating is necessary, but node framework logic should still be placed in the `node`
module, not scattered across the crate.

[`zksync_server`]: ../../bin/zksync_server
[`zksync_external_node`]: ../../bin/external_node

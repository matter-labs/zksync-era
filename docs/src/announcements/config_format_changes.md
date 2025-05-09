# Configuration Format Changes

## Overview

As of April 2025, ZKsync nodes are transitioning to
[the new multi-layered configuration system](https://github.com/matter-labs/smart-config) developed in-house as a Rust
library (usable for other apps as well!).

The reasons for this transition are multifold; the main ones include:

- Flexible distribution of configuration sources. Currently, node support either loading configuration _only_ from env
  variables, or from a predefined set of YAML files, each with a fixed schema. With the new system, it will be possible
  to arbitrarily distribute params among YAML / JSON files, and/or env variables. Redefinitions will be resolved via
  well-defined source priorities.
- Unification of parameter naming / schema for env-based and file-based config sources. Currently, some params have a
  differing name and/or schema depending on the source.
- Reasonable defaults for the majority of config parameters. Currently, the vast majority of parameters are required.
- Auto-generated documentation for config params embedded directly into the node executable.

There are more; see the
[config system readme](https://github.com/matter-labs/smart-config/tree/main/crates/smart-config#readme) for a more
complete list of features.

## Migration path

Migration to the new system will proceed in stages. On the initial stage, the node CLI options related to the config
will not change (i.e., a node will still read the config either from env vars, or from a list of YAML files). The vast
majority of the config schema is backward-compatible, both for file-based and env-based formats, with a couple of
exceptions described below.

```admonish warning
The env-based format is *de facto* currently deprecated for the main node (the `zksync_server` binary; launched via `zkstack server`).
Potential breaking changes in it are not exhaustively tested and will be fixed at best-effort basis.
```

The initial stage will also not touch env-based config for non-validator nodes (`zksync_external_node` binary); it will
be migrated separately.

Migration is only necessary for _existing_ configurations; config files newly created via (updated) `zkstack` will have
the necessary changes applied automatically.

### Tagged enumerations

The current schema uses "one-of" encoding for enumerations, in which an enumeration is represented as a single-field
object with the key corresponding to the variant, and value being the variant payload. With the change, the tagged
representation will be used instead, in which the tag is placed in a separate key _adjacent_ to the payload. (Tagged
representation works better with prioritized merging.)

The following enumeration configs are affected.

#### Object stores

Object store configs at `snapshot_creator.object_store`, `prover.prover_object_store`, `snapshot_recovery.object_store`,
and `core_object_store` in `general.yaml`.

As an example, take the following configuration before migration:

```yaml
core_object_store:
  file_backed:
    file_backed_base_path: artifacts
  max_retries: 10
```

In the new format, it will look as follows:

```yaml
core_object_store:
  mode: FileBacked
  file_backed_base_path: artifacts
  max_retries: 10
```

I.e., the object beneath the `file_backed` tag is unindented, and the tag itself is transformed to `mode: FileBacked`.
It is possible to define a config that will be parsed both by old and new node versions; this is recommended for
no-downtime migration.

```yaml
core_object_store:
  # Read by old node versions
  file_backed:
    file_backed_base_path: artifacts
  # Read by new node versions
  mode: FileBacked
  file_backed_base_path: artifacts
  # Read by both
  max_retries: 10
```

#### DA client

DA client configs at `da_client` in `general.yaml` and at `da` in `secrets.yaml`. Additionally, some clients have
additional enumerations:

- If the Avail DA client is configured, `da_client.avail` specifies the client subtype: `full_client` or `gas_relay`.
- If the Eigen DA client is configured, `da_client.eigen` specifies the EC points source: `points_source_url` or
  `points_source_path`.

As an example, take the following fragment of `general.yaml` before migration:

```yaml
da_client:
  avail:
    bridge_api_url: https://turing-bridge-api.avail.so/
    timeout_ms: 10000
    gas_relay:
      gas_relay_api_url: https://example.com/
      max_retries: 7
```

In the new format, 2 enumerations will be changed: one at `da_client` (acquiring tag `client`) and at
`da_client.avail.gas_relay` (acquiring tag `avail_client_type`):

```yaml
da_client:
  bridge_api_url: https://turing-bridge-api.avail.so/
  timeout_ms: 10000
  avail_client_type: GasRelay
  gas_relay_api_url: https://example.com/
  max_retries: 7
  client: Avail
```

As in the previous case, it is possible to define a config that will be parsed both by old and new node versions by
deep-merging the old and new configs.

An example for the Eigen DA client:

```yaml
# Before migration
da_client:
  eigen:
    disperser_rpc: https://disperser-holesky.eigenda.xyz:443
    settlement_layer_confirmation_depth: 0
    eigenda_eth_rpc: https://holesky.infura.io/v3/CORRECT_HORSE
    eigenda_svc_manager_address: 0xD4A7E1Bd8015057293f0D0A557088c286942e84b
    wait_for_finalization: true
    authenticated: true
    points_source_url:
      g1_url: https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g1.point
      g2_url: https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g2.point.powerOf2
---
# After migration
da_client:
  client: Eigen
  disperser_rpc: https://disperser-holesky.eigenda.xyz:443
  settlement_layer_confirmation_depth: 0
  eigenda_eth_rpc: https://holesky.infura.io/v3/CORRECT_HORSE
  eigenda_svc_manager_address: 0xD4A7E1Bd8015057293f0D0A557088c286942e84b
  wait_for_finalization: true
  authenticated: true
  points:
    source: Url
    g1_url: https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g1.point
    g2_url: https://raw.githubusercontent.com/lambdaclass/zksync-eigenda-tools/6944c9b09ae819167ee9012ca82866b9c792d8a1/resources/g2.point.powerOf2
```

For `secrets.yaml`, the change is as follows:

```yaml
# Before migration
da:
  avail:
    seed_phrase: correct horse battery staple
---
# After migration
da:
  client: Avail
  seed_phrase: correct horse battery staple
```

It is recommended (although not strictly necessary) to also rename `da` in `secrets.yaml` to `da_client` (or copy if
no-downtime migration is required).

#### Deployment allowlist

Configuration at `state_keeper.deployment_allowlist`. The tag field name in this case is `source`:

```yaml
# Before migration
state_keeper:
  deployment_allowlist:
    dynamic:
      http_file_url: http://deployment_allowlist/
      refresh_interval_secs: 60
---
# After migration
state_keeper:
  deployment_allowlist:
    source: Dynamic
    http_file_url: http://deployment_allowlist/
    refresh_interval_secs: 60
```

### Other changes

- `snapshot_recovery.experimental.drop_storage_key_preimages` parameter, if present, should be moved / copied to
  `snapshot_recovery.drop_storage_key_preimages`.
- Likewise, `snapshot_recovery.experimental.tree_recovery_parallel_persistence_buffer`, if present, should be moved /
  copied to `snapshot_recovery.tree.parallel_persistence_buffer`.

## Debugging configuration

To debug configuration parameters, launch the node with the `config` command, e.g. `zksync_server config` or
`zkstack server -- config` if using `zkstack`. By default, the `config` command will output help on configuration
params, optionally filtered by the param name. If the `--debug` flag is specified, param values will be output instead.

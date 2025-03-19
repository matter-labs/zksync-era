# EigenDA Client

EigenDA is as a high-throughput data availability layer for rollups. It is an EigenLayer AVS (Actively Validated
Service), so it leverages Ethereum's economic security instead of bootstrapping a new network with its own validators.
For more information you can check the [docs](https://docs.eigenda.xyz/).

## Client configuration

First you need to set the `use_dummy_inclusion_data` field in the file `etc/env/file_based/general.yaml` to `true`. This
is a pending solution until the necessary contract changes are done (M1 milestone).

```yaml
da_dispatcher:
  use_dummy_inclusion_data: true
```

The client can be set up by modifying the field `da_client` of the file `etc/env/file_based/overrides/validium.yaml`.
These are the fields that can be modified:

- `disperser_rpc` (string): URL of the EigenDA Disperser RPC server.
- `settlement_layer_confirmation_depth` (unsigned number): Block height needed to reach in order to consider the blob
  finalized. A value less or equal to 0 means that the disperser will not wait for finalization.
- `eigenda_eth_rpc` (optional string): URL of the Ethereum RPC server. If the value is not set, the client will use the
  same rpc as the rest of the zk server.
- `eigenda_svc_manager_address` (string): Address of the service manager contract.
- `wait_for_finalization` (boolean): Wait for the blob to be finalized before returning the response.
- `authenticated` (boolean): Authenticated dispersal. If true, the client will use the authentication mechanism, using a
  whitelisted account. Using non authenticated dispersal is not recommended, as to many requests to the EigenDA
  disperser leeds to timeouts. (the following two fields are mutually exclusive)
- `points_source_path` (string): Path to the local points source files.
- `points_source_url` (string): URLs of the points source files.
- `custom_quorum_numbers` (optional list of numbers): quorums to be used beside the default ones.

So, for example, a client setup that uses the holesky EigenDA client would look like this:

```yaml
eigen:
  disperser_rpc: https://disperser-holesky.eigenda.xyz:443
  settlement_layer_confirmation_depth: 0
  eigenda_eth_rpc: https://ethereum-holesky-rpc.publicnode.com
  eigenda_svc_manager_address: 0xD4A7E1Bd8015057293f0D0A557088c286942e84b
  wait_for_finalization: false
  authenticated: false
  points_source_url:
    g1_url: https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point
    g2_url: https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2
  # custom_quorum_numbers: 2,3 # uncomment to use other quorums besides defaults
```

If using `authenticated` dispersal, you also need to modify `etc/env/file_based/secrets.yaml` to include the private key
of the account that will be used. You need to add the following field:

```yaml
da:
  eigen:
    private_key: <PRIVATE_KEY>
```

Note: the private key should be in hex format, without the `0x` prefix.

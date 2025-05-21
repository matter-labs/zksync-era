# EigenDA Client

EigenDA is as a high-throughput data availability layer for rollups. It is an EigenLayer AVS (Actively Validated
Service), so it leverages Ethereum's economic security instead of bootstrapping a new network with its own validators.
For more information you can check the [docs](https://docs.eigenda.xyz/).

## Client configuration

To set up an eigenda client, you need to modify `etc/env/file_based/overrides/validium.yaml`:

First you need to set the `use_dummy_inclusion_data` field to `true`. This is a pending solution until the necessary
contract changes are done (M1 milestone).

```yaml
da_dispatcher:
  use_dummy_inclusion_data: true
```

Then set up the client by modifying the field `da_client`, the following fields are common for both V1 and V2:

- `disperser_rpc` (string): URL of the EigenDA Disperser RPC server.
- `eigenda_eth_rpc` (optional string): URL of the Ethereum RPC server. If the value is not set, the client will use the
  same rpc as the rest of the zk server.
- `authenticated` (boolean): Authenticated dispersal. If true, the client will use the authentication mechanism, using a
  whitelisted account. Using non authenticated dispersal is not recommended, as to many requests to the EigenDA
  disperser leeds to timeouts.

This is an example of how the config should be looking like:

```yaml
da_client:
  client: EigenDA
  disperser_rpc: https://disperser-testnet-holesky.eigenda.xyz #Under DISPERSER_RPC env variable
  eigenda_eth_rpc: https://ethereum-holesky-rpc.publicnode.com #Under RPC_URL env variable
  authenticated: true
```

If using `authenticated` dispersal, you also need to modify `etc/env/file_based/secrets.yaml` to include the private key
of the account that will be used. You need to add the following field:

```yaml
da_client:
  client: EigenDA
  private_key: <PRIVATE_KEY>
```

> Note: the private key should be in hex format, without the `0x` prefix.

Now continue with the configuration with v1 or v2 specifics:

### V1 Specific client configuration

A V1 client is configured by adding the `client_type: v1` field to the `da_client`, these are the fields that can be
modified:

- `settlement_layer_confirmation_depth` (unsigned number): Block height needed to reach in order to consider the blob
  finalized. A value less or equal to 0 means that the disperser will not wait for finalization.
- `eigenda_svc_manager_address` (string): Address of the service manager contract.
- `wait_for_finalization` (boolean): Wait for the blob to be finalized before returning the response.
- `points`
  - `source`: `Url` or `Path`
  - if `Url`
    - `g1_url`: (string): URLs of the g1 point source file.
    - `g2_url`: (string): URLs of the g2 point source file.
  - if `Path`
    - `path`: (string): Path to the local points source files.
- `custom_quorum_numbers` (optional list of numbers): quorums to be used beside the default ones.

So, for example, a client setup that uses the holesky EigenDA V1 client would look like this:

`etc/env/file_based/overrides/validium.yaml`:

```yaml
da_dispatcher:
  use_dummy_inclusion_data: true
da_client:
  client: EigenDA
  disperser_rpc: https://disperser-testnet-holesky.eigenda.xyz
  eigenda_eth_rpc: https://ethereum-holesky-rpc.publicnode.com
  authenticated: true
  client_type:
    version: V1
    settlement_layer_confirmation_depth: 0
    eigenda_svc_manager_address: 0xD4A7E1Bd8015057293f0D0A557088c286942e84b
    wait_for_finalization: false
    points:
      source: Url
      g1_url: https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point
      g2_url: https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2
      # source: Path # uncomment to use Path
      # path: ./resources
    # custom_quorum_numbers: 2,3 # uncomment to use other quorums besides defaults
```

### V2 specific client configuration

A V2 client is configured by adding the `client_type: v2` field to the `da_client`, these are the fields that can be
modified:

- `cert_verifier_addr` Address of the eigenDA cert verifier contract
- `blob_version` Blob Version used by eigenDA, currently only blob version 0 is supported
- `polynomial_form` Polynomial form used to encode data, either coeff or eval

So, for example, a client setup that uses the holesky EigenDA V2 client would look like this:

```yaml
da_dispatcher:
  use_dummy_inclusion_data: true
da_client:
  client: EigenDA
  disperser_rpc: https://disperser-testnet-holesky.eigenda.xyz
  eigenda_eth_rpc: https://ethereum-holesky-rpc.publicnode.com
  authenticated: true
  client_type:
    version: V2
    cert_verifier_addr: 0xfe52fe1940858dcb6e12153e2104ad0fdfbe1162
    blob_version: 0
    polynomial_form: coeff
```

# Migration from V1 to V2 client

If you updated your version of zksync-era, and your EigenDA config looks something like this:

`chains/<YOUR_CHAIN>/configs/general.yaml`

```yaml
da_client:
  client: Eigen
  disperser_rpc: https://disperser-testnet-holesky.eigenda.xyz
  eigenda_eth_rpc: https://ethereum-holesky-rpc.publicnode.com
  authenticated: true
  settlement_layer_confirmation_depth: 0
  eigenda_svc_manager_address: 0xD4A7E1Bd8015057293f0D0A557088c286942e84b
  wait_for_finalization: false
  points:
    source: Url
    g1_url: https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point
    g2_url: https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2
  custom_quorum_numbers: 2,3
```

You need to update the client config to the new format:

### If you want to migrate to V2

`chains/<YOUR_CHAIN>/configs/general.yaml`

```yaml
da_client:
  client: EigenDA
  disperser_rpc: https://disperser-testnet-holesky.eigenda.xyz
  eigenda_eth_rpc: https://ethereum-holesky-rpc.publicnode.com
  authenticated: true
  client_type:
    version: V2
    cert_verifier_addr: 0xfe52fe1940858dcb6e12153e2104ad0fdfbe1162
    blob_version: 0
    polynomial_form: coeff #Either coeff or eval
```

### If you prefer to stay on V1

`chains/<YOUR_CHAIN>/configs/general.yaml`

```yaml
da_client:
  client: EigenDA
  disperser_rpc: https://disperser-testnet-holesky.eigenda.xyz
  eigenda_eth_rpc: https://ethereum-holesky-rpc.publicnode.com
  authenticated: true
  client_type:
    version: V1
    settlement_layer_confirmation_depth: 0
    eigenda_svc_manager_address: 0xD4A7E1Bd8015057293f0D0A557088c286942e84b
    authenticated: false
    wait_for_finalization: false
    points:
      source: Url
      g1_url: https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point
      g2_url: https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2
    custom_quorum_numbers: 2,3
```

Check the [README.md](./README.md) for more details on the new fields.

### Note

You should also change your `chains/<YOUR_CHAIN>/configs/secrets.yaml` from:

```yaml
da_client:
  client: Eigen
  private_key: <your_private_key>
```

To

```yaml
da_client:
  client: EigenDA
  private_key: <your_private_key>
```

- Be sure that your private key has the needed permissions set on the V2 client.

- Remember to run `zkstackup --local` before running the new server after this changes

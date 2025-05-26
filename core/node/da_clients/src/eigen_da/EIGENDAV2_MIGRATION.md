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
  # custom_quorum_numbers: 2,3
```

You need to update the client config to the new format, this is done by adding the new fields for V2, and specifying the
version to use:

`chains/<YOUR_CHAIN>/configs/general.yaml`

```yaml
da_client:
  client: EigenDA
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
  # custom_quorum_numbers: 2,3
  # Add this new fields:
  version: V2
  cert_verifier_addr: 0xfe52fe1940858dcb6e12153e2104ad0fdfbe1162
  blob_version: 0
  polynomial_form: coeff #Either coeff or eval
  eigenda_sidecar_rpc: http://localhost:9999
```

Note that the client changed from `Eigen` to `EigenDA`

Whether you are using `V1` or `V2` you still need to specify the fields for the other version. Why do you need to keep
both config parameters? The idea is to eventually remove the V1 version entirely, with the config like this, this will
be seamless, since you would only need to remove the V1 specific fields.

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

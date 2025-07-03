# Migration from V1 to V2 client

Before you update your version of zksync-era you should put values needed for both version of the config; after you
update you can remove old config values.

So if your config looked like:

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

You need to add the new fields for V2:

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
  cert_verifier_router_addr: 0xdd735affe77a5ed5b21ed47219f95ed841f8ffbd
  operator_state_retriever_addr: 0xB4baAfee917fb4449f5ec64804217bccE9f46C67
  registry_coordinator_addr: 0x53012C69A189cfA2D9d29eb6F19B32e0A2EA3490
  blob_version: 0
```

After you update, you can remove some of the old V1 fields, leaving you with:

`chains/<YOUR_CHAIN>/configs/general.yaml`

```yaml
da_client:
  client: Eigen
  disperser_rpc: https://disperser-testnet-holesky.eigenda.xyz
  eigenda_eth_rpc: https://ethereum-holesky-rpc.publicnode.com
  cert_verifier_router_addr: 0xdd735affe77a5ed5b21ed47219f95ed841f8ffbd
  operator_state_retriever_addr: 0xB4baAfee917fb4449f5ec64804217bccE9f46C67
  registry_coordinator_addr: 0x53012C69A189cfA2D9d29eb6F19B32e0A2EA3490
  blob_version: 0
```

> Note: There is a missing field, `eigenda_sidecar_rpc`, this determines wheter the client will use secure mode or not.

Check the [README.md](./README.md) for more details on the new fields.

### Note

- Remember to run `zkstackup --local` before running the new server after this changes

- Some form of [payment](https://docs.eigencloud.xyz/products/eigenda/core-concepts/payments) needs to have been performed onchain for your private key 

- You can find the addresses for mainnet [here](https://docs.eigencloud.xyz/products/eigenda/networks/mainnet)

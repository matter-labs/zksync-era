# EigenDA Client

EigenDA is as a high-throughput data availability layer for rollups. It is an EigenLayer AVS (Actively Validated
Service), so it leverages Ethereum's economic security instead of bootstrapping a new network with its own validators.
For more information you can check the [docs](https://docs.eigenda.xyz/).

## Client configuration

To set up an eigenda client, you need to modify `etc/env/file_based/overrides/validium.yaml`:

Then set up the client by modifying the field `da_client`, add the following fields:

- `disperser_rpc` (string): URL of the EigenDA Disperser RPC server.
- `eigenda_eth_rpc` (optional string): URL of the Ethereum RPC server. If the value is not set, the client will use the
  same rpc as the rest of the zk server.
- `cert_verifier_router_addr` Address of the eigenDA cert verifier router contract
- `operator_state_retriever_addr` Address of the Eigen operator state retriever contract
- `registry_coordinator_addr` Address of the Eigen registry coordinator contract
- `blob_version` Blob Version used by eigenDA, currently only blob version 0 is supported

You also need to modify `etc/env/file_based/secrets.yaml` to include the private key of the account that will be used.
You need to add the following field:

```yaml
da_client:
  client: Eigen
  private_key: <PRIVATE_KEY>
```

> Note: the private key should be in hex format, without the `0x` prefix.

### V2 client configuration

A client setup that uses the holesky EigenDA non-secure V2 client would look like this:

`etc/env/file_based/overrides/validium.yaml`:

```yaml
da_dispatcher:
  use_dummy_inclusion_data: true
da_client:
  client: Eigen
  disperser_rpc: https://disperser-testnet-holesky.eigenda.xyz
  eigenda_eth_rpc: https://ethereum-holesky-rpc.publicnode.com
  cert_verifier_router_addr: 0xdd735affe77a5ed5b21ed47219f95ed841f8ffbd
  operator_state_retriever_addr: 0xB4baAfee917fb4449f5ec64804217bccE9f46C67
  registry_coordinator_addr: 0x53012C69A189cfA2D9d29eb6F19B32e0A2EA3490
  blob_version: 0
```

### V2Secure specific client configuration

A V2 Secure client uses the same fields as the `V2` Version, and adds a new field:

- `eigenda_prover_service_rpc` RPC of the EigenDA prover service that generates the proofs

And a client setup that uses the holesky EigenDA V2 Secure client would look like this:

`etc/env/file_based/overrides/validium.yaml`:

```yaml
da_dispatcher:
  use_dummy_inclusion_data: false
da_client:
  client: Eigen
  disperser_rpc: https://disperser-testnet-holesky.eigenda.xyz
  eigenda_eth_rpc: https://ethereum-holesky-rpc.publicnode.com
  cert_verifier_router_addr: 0xdd735affe77a5ed5b21ed47219f95ed841f8ffbd
  operator_state_retriever_addr: 0xB4baAfee917fb4449f5ec64804217bccE9f46C67
  registry_coordinator_addr: 0x53012C69A189cfA2D9d29eb6F19B32e0A2EA3490
  blob_version: 0
  eigenda_prover_service_rpc: http://localhost:9999
```

> Note: The `eigenda_prover_service_rpc` field determines wheter the client will use secure mode or not.

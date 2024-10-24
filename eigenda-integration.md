# Zksync-era <> EigenDA Integration

EigenDA is as a high-throughput data availability layer for rollups. It is an EigenLayer AVS (Actively Validated
Service), so it leverages Ethereum's economic security instead of bootstrapping a new network with its own validators.
For more information you can check the [docs](https://docs.eigenda.xyz/).

## Scope

The scope of this first milestone is to spin up a local EigenDA dev environment, spin up a local zksync-era dev
environment and integrate them. Instead of sending 4844 blobs, the zksync-era sends blobs to EigenDA. EigenDA provides a
high level client called [eigenda-proxy](https://github.com/Layr-Labs/eigenda-proxy), and it is used to communicate with
the EigenDA disperser in a secury and easy way. On L1, mock the verification logic, such that blocks continue building.
Increase the blob size from 4844 size to 2MiB blob. Deploy the integration to Holesky testnet and provide scripts to
setup a network using EigenDA as DA provider.

## Common changes

Changes needed both for local and mainnet/testnet setup.

1. Add `da_client` to `etc/env/file_based/general.yaml`:

If you want to use memstore:

```yaml
da_client:
  eigen_da:
    memstore:
      api_node_url: http://127.0.0.1:4242 # TODO: This should be removed once eigenda proxy is no longer used
      max_blob_size_bytes: 2097152
      blob_expiration: 100000
      get_latency: 100
      put_latency: 100
```

If you want to use disperser:

```yaml
da_client:
  eigen_da:
    disperser:
      api_node_url: http://127.0.0.1:4242 # TODO: This should be removed once eigenda proxy is no longer used
      disperser_rpc: <your_desired_disperser>
      eth_confirmation_depth: -1
      eigenda_eth_rpc: <your_desired_rpc>
      eigenda_svc_manager_addr: '0xD4A7E1Bd8015057293f0D0A557088c286942e84b'
      blob_size_limit: 2097152
      status_query_timeout: 1800
      status_query_interval: 5
      wait_for_finalization: false
```

2. Add `eigenda-proxy` to the `docker-compose.yml` file:

```yaml
eigenda-proxy:
  image: ghcr.io/layr-labs/eigenda-proxy
  ports:
    - '4242:4242'
  command: ./eigenda-proxy --addr 0.0.0.0 --port 4242 --memstore.enabled --eigenda-max-blob-length "2MiB"
```

3. (optional) for using pubdata with 2MiB (as per specification), modify `etc/env/file_based/general.yaml`:

```yaml
max_pubdata_per_batch: 2097152
```

## Local Setup

1. Install `zk_inception` & `zk_supervisor`

```bash
./bin/zkt
```

2. Start containers

```bash
zk_inception containers --observability true
```

3. Add EigenDA Dashboard

```bash
mv era-observability/additional_dashboards/EigenDA.json era-observability/dashboards/EigenDA.json
```

3. Create `eigen_da` chain

```bash
zk_inception chain create \
          --chain-name eigen_da \
          --chain-id sequential \
          --prover-mode no-proofs \
          --wallet-creation localhost \
          --l1-batch-commit-data-generator-mode validium \
          --base-token-address 0x0000000000000000000000000000000000000001 \
          --base-token-price-nominator 1 \
          --base-token-price-denominator 1 \
          --set-as-default false
```

4. Initialize created ecosystem

```bash
zk_inception ecosystem init \
          --deploy-paymaster true \
          --deploy-erc20 true \
          --deploy-ecosystem true \
          --l1-rpc-url http://127.0.0.1:8545 \
          --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
          --server-db-name=zksync_server_localhost_eigen_da \
          --chain eigen_da \
          --verbose
```

You may enable observability here if you want to.

5. Start the server

```bash
zk_inception server --chain eigen_da
```

### Testing

Modify the following flag in `core/lib/config/src/configs/da_dispatcher.rs` (then restart the server)

```rs
pub const DEFAULT_USE_DUMMY_INCLUSION_DATA: bool = true;
```

And with the server running on one terminal, you can run the server integration tests on a separate terminal with the
following command:

```bash
zk_supervisor test integration --chain eigen_da
```

### Metrics

Access Grafana at [http://localhost:3000/](http://localhost:3000/), go to dashboards and select `EigenDA`.

## Mainnet/Testnet setup

### Modify localhost chain id number

Modify line 32 in `zk_toolbox/crates/types/src/l1_network.rs`:

```rs
L1Network::Localhost => 17000,
```

Then recompile the zk toolbox:

```bash
./bin/zkt
```

### Used wallets

Modify `etc/env/file_based/wallets.yaml` and `configs/wallets.yaml` with the following wallets:

```yaml
# Use your own holesky wallets, be sure they have enough funds
```

> ⚠️ Some steps distribute ~5000ETH to some wallets, modify `AMOUNT_FOR_DISTRIBUTION_TO_WALLETS` to a lower value if
> needed.

### EigenProxy RPC

Get `EIGEN_SIGNER_PK` from 1password and set it as an `env` var:

```bash
export EIGEN_SIGNER_PK=<VALUE_HERE>
export HOLESKY_RPC_URL=<VALUE_HERE>
```

Modify `docker-compose.yml` to use holesky RPCs:

```rust
  eigenda-proxy:
    image: ghcr.io/layr-labs/eigenda-proxy
    environment:
      - EIGEN_SIGNER_PK=$EIGEN_SIGNER_PK
      - HOLESKY_RPC_URL=$HOLESKY_RPC_URL
    ports:
      - "4242:4242"
    command: ./eigenda-proxy --addr 0.0.0.0 --port 4242 --eigenda-disperser-rpc disperser-holesky.eigenda.xyz:443 --eigenda-signer-private-key-hex $EIGEN_SIGNER_PK --eigenda-eth-rpc $HOLESKY_RPC_URL --eigenda-svc-manager-addr 0xD4A7E1Bd8015057293f0D0A557088c286942e84b --eigenda-eth-confirmation-depth 0
```

### Create and initialize the ecosystem

(be sure to have postgres container running on the background)

```bash
zk_inception chain create \
          --chain-name holesky_eigen_da \
          --chain-id 114411 \
          --prover-mode no-proofs \
          --wallet-creation localhost \
          --l1-batch-commit-data-generator-mode validium \
          --base-token-address 0x0000000000000000000000000000000000000001 \
          --base-token-price-nominator 1 \
          --base-token-price-denominator 1 \
          --set-as-default false

zk_inception ecosystem init \
          --deploy-paymaster true \
          --deploy-erc20 true \
          --deploy-ecosystem true \
          --l1-rpc-url $HOLESKY_RPC_URL \
          --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
          --server-db-name=zksync_server_holesky_eigen_da \
          --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
          --prover-db-name=zksync_prover_holesky_eigen_da \
          --chain holesky_eigen_da \
          --verbose
```

### Start the server

```bash
zk_inception server --chain holesky_eigen_da
```

## Backup and restoration

It's possible to run the zk stack on one computer, and then migrate it to another, this is specially useful for holesky
testing.

### Backup

Suppose that you want to make a backup of `holesky_eigen_da` ecosystem, you only need to run:

```bash
./backup-ecosystem.sh holesky_eigen_da
```

This will generate a directory inside of `ecosystem_backups` with the name `holesky_eigen_da`.

### Restoration

1. Move the `ecoystem_backups/holesky_eigen_da` directory to the other computer, it should be placed in the root of the
   project.

2. Restore the ecosystem with:

```bash
./restore-ecosystem.sh holesky_eigen_da
```

Note that:

- The `postgres` container has to be running.
- The `chain_id` can't be already in use.
- If you are restoring a local ecosystem, you have to use the same `reth` container as before.
- If no ecosystem has been `init`ialized on this computer before, run this command:

```bash
git submodule update --init --recursive && zk_supervisor contracts
```

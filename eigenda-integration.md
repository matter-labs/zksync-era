# Zksync-era <> EigenDA Integration

## Local Setup

1. Install `zk_inception` & `zk_supervisor`

```bash
./bin/zkt
```

2. Start containers

```bash
zk_inception containers --observability true
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
          --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
          --prover-db-name=zksync_prover_localhost_eigen_da \
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

## Holesky Setup

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

> ⚠️ Some steps distribute ~5000ETH to some wallets, modify `AMOUNT_FOR_DISTRIBUTION_TO_WALLETS` to a lower value if needed.

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

It's possible to run the zk stack on one computer, and then migrate it to another, this is specially useful for holesky testing.

### Backup

Suppose that you want to make a backup of `holesky_eigen_da` ecosystem, you only need to run:

```bash
./backup-ecosystem.sh holesky_eigen_da
```

This will generate a directory inside of `ecosystem_backups` with the name `holesky_eigen_da`.

### Restoration

1. Move the `ecoystem_backups/holesky_eigen_da` directory to the other computer, it should be placed in the root of the project.

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

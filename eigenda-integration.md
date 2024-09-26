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

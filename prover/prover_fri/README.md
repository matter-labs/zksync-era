# FRI Prover

## running cpu prover

`zk f cargo +nightly-2023-05-31 run --release --bin zksync_prover_fri`

## running gpu prover(requires CUDA 12.0+)

`zk f cargo +nightly-2023-05-31 run --release -features "gpu" --bin zksync_prover_fri`

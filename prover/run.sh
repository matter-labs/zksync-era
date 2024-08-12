zk f cargo run --release --bin zksync_witness_generator -- --all_rounds

zk f cargo run --release --bin zksync_witness_vector_generator -- --threads 10

zk f cargo run --features "gpu" --release --bin zksync_prover_fri

BELLMAN_CUDA_DIR=$PWD/bellman-cuda zk f cargo run --features "gpu" --release --bin zksync_proof_fri_compressor

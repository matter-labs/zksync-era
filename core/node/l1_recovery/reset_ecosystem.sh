clear
cd ../../..
cargo run --bin local_blobs_dump --release -- \
  --config-path chains/era/configs/general.yaml \
  --secrets-path chains/era/configs/secrets.yaml
docker compose --file docker-compose.yml down --volumes
zkstack containers --observability
rm -rf chains/era/db/main
zkstack ecosystem init \
  --dev \
  --deploy-paymaster \
  --deploy-erc20 \
  --deploy-ecosystem \
  --l1-rpc-url=http://localhost:8545 \
  --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
  --server-db-name=zksync_server_localhost_era \
  --ignore-prerequisites \
  --verbose \
  --observability=false

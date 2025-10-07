clear
cd core
cargo run --bin local_blobs_dump --release -- \
  --config-path ../chains/era/configs/general.yaml \
  --secrets-path ../chains/era/configs/secrets.yaml
zkstack dev db reset
rm -rf ../chains/era/db/main
zkstack server --l1-recovery --verbose

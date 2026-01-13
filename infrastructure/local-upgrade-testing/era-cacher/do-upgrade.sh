era-cacher/reset.sh

era-cacher/use-old-era.sh && cd zksync-working

# We delete information about the gateway chain as presence of old chain configs
# changes the beavior of zkstack ecosystem init.
[ -d "./chains/gateway" ] && rm -rf "./chains/gateway"

zkstackup --local && zkstack dev clean containers && zkstack up --observability false

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --verbose \
    --observability=false

zkstack chain create \
    --chain-name gateway \
    --chain-id 505 \
    --prover-mode no-proofs \
    --wallet-creation localhost \
    --l1-batch-commit-data-generator-mode rollup \
    --base-token-address 0x0000000000000000000000000000000000000001 \
    --base-token-price-nominator 1 \
    --base-token-price-denominator 1 \
    --set-as-default false \
    --evm-emulator false \
    --ignore-prerequisites

zkstack chain init \
    --deploy-paymaster \
    --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_gateway \
    --chain gateway

zkstack chain gateway convert-to-gateway --chain gateway
# When running locally, it makes sense to not redirect to file, but start a new terminal window here.
zkstack server --chain gateway &> ../gateway.log &

# When running locally, open a new terminal window here.
zkstack chain gateway migrate-to-gateway --chain era --gateway-chain-name gateway
# When running locally, it makes sense to not redirect to file, but start a new terminal window during the next operation.
zkstack server --chain era &> ../rollup.log &

pkill -9 zksync_server

# When running locally, open a new terminal window here.
cd .. && era-cacher/use-new-era.sh && cd zksync-working

# If you are running locally, ensure to go out and back to the zksync-working directory, i.e.
# `cd ..` and then `cd zksync-working`.
# Sometimes, the console may not reflect the changes made to the codebase.

zkstackup --local
zkstack dev contracts

cd contracts
git checkout sb-v29-upgrade-testing
cd ..

zkstack dev database migrate --prover false --core true  --chain era
zkstack dev database migrate --prover false --core true  --chain gateway

# All the actions below may be performed in a different window.
zkstack server --ignore-prerequisites --chain gateway &> ../gateway.log &
zkstack server --ignore-prerequisites --chain era &> ../rollup.log &

echo "Server started"

zkstack dev run-ecosystem-upgrade --upgrade-version v29-interop-a-ff --ecosystem-upgrade-stage no-governance-prepare
zkstack dev run-ecosystem-upgrade --upgrade-version v29-interop-a-ff --ecosystem-upgrade-stage ecosystem-admin
zkstack dev run-ecosystem-upgrade --upgrade-version v29-interop-a-ff --ecosystem-upgrade-stage governance-stage0
zkstack dev run-ecosystem-upgrade --upgrade-version v29-interop-a-ff --ecosystem-upgrade-stage governance-stage1

cd contracts/l1-contracts
UPGRADE_ECOSYSTEM_OUTPUT=script-out/v29-upgrade-ecosystem.toml UPGRADE_ECOSYSTEM_OUTPUT_TRANSACTIONS=broadcast/EcosystemUpgrade_v29.s.sol/9/run-latest.json  YAML_OUTPUT_FILE=script-out/v29-local-output.yaml yarn upgrade-yaml-output-generator
cd ../../

zkstack dev run-v29-chain-upgrade  --force-display-finalization-params=true --dangerous-local-default-overrides=true --chain era 
zkstack dev run-v29-chain-upgrade  --force-display-finalization-params=true --dangerous-local-default-overrides=true --chain gateway
zkstack dev run-ecosystem-upgrade --upgrade-version v29-interop-a-ff --ecosystem-upgrade-stage governance-stage2

pkill -9 zksync_server
zkstack server --ignore-prerequisites --chain era &> ../rollup2.log &

sleep 10

zkstack dev test integration --no-deps --ignore-prerequisites --chain era

era-cacher/reset.sh

era-cacher/use-old-era.sh && cd zksync-working

zkstackup -g --local && zkstack dev clean containers && zkstack up --observability false

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --verbose \
    --observability=false

## kl todo start chain here, turn it off.

cd .. && era-cacher/use-new-era.sh && cd zksync-working

zkstackup -g --local --cargo-features upgrades
zkstack dev contracts

cd contracts
git checkout vg/v29-upgrade-script-fixes
cd ..

zkstack dev database migrate --prover false --core true 

# # zkstack chain gateway-upgrade -- adapt-config

# Server should be started in a different window for consistency
zkstack server --ignore-prerequisites --chain era &> ../rollup.log &
echo "Server started"

zkstack dev run-ecosystem-upgrade --upgrade-version v28_1_vk --ecosystem-upgrade-stage no-governance-prepare

zkstack dev run-ecosystem-upgrade --upgrade-version v28_1_vk --ecosystem-upgrade-stage governance-stage0

zkstack  dev run-ecosystem-upgrade --upgrade-version v28_1_vk --ecosystem-upgrade-stage governance-stage1

cd contracts/l1-contracts
UPGRADE_ECOSYSTEM_OUTPUT=script-out/v29-upgrade-ecosystem.toml UPGRADE_ECOSYSTEM_OUTPUT_TRANSACTIONS=broadcast/EcosystemUpgrade_v29.s.sol/9/run-latest.json  YAML_OUTPUT_FILE=script-out/v29-local-output.yaml yarn upgrade-yaml-output-generator
cd ../../

zkstack dev run-v29-chain-upgrade 

zkstack  dev run-ecosystem-upgrade --upgrade-version v28_1_vk --ecosystem-upgrade-stage governance-stage2

pkill -9 zksync_server
zkstack server --ignore-prerequisites --chain era &> ../rollup2.log &

sleep 10

zkstack dev test integration --no-deps --ignore-prerequisites --chain era

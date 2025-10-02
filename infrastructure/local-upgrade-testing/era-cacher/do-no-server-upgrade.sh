# era-cacher/reset.sh

# era-cacher/use-new-era.sh && cd zksync-working

upgrade_version="v29-interop-a-ff"
# "v28-1-vk"
upgrade_file_extension="v29"
# v28-1-zk-os


zkstackup  --local --cargo-features upgrades && zkstack dev clean containers && zkstack up --observability false
zkstack dev contracts

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --verbose \
    --observability=false \
    --validium-type no-da \
    --update-submodules false 

# Server should be started in a different window for consistency
zkstack server --ignore-prerequisites --chain era &> ../rollup.log &
echo "Server started"

zkstack dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage no-governance-prepare --update-submodules false

zkstack dev run-ecosystem-upgrade  --upgrade-version $upgrade_version --ecosystem-upgrade-stage governance-stage0 --update-submodules false

zkstack  dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage governance-stage1 --update-submodules false

cd contracts/l1-contracts
UPGRADE_ECOSYSTEM_OUTPUT=script-out/$upgrade_file_extension-upgrade-ecosystem.toml \
UPGRADE_ECOSYSTEM_OUTPUT_TRANSACTIONS=broadcast/EcosystemUpgrade_$upgrade_file_extension.s.sol/9/run-latest.json  \
YAML_OUTPUT_FILE=script-out/$upgrade_file_extension-local-output.yaml yarn upgrade-yaml-output-generator
cd ../../

zkstack dev run-chain-upgrade --upgrade-version $upgrade_version

zkstack  dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage governance-stage2

pkill -9 zksync_server
zkstack server --ignore-prerequisites --chain era &> ../rollup2.log &

sleep 10

zkstack dev test integration --no-deps --ignore-prerequisites --chain era
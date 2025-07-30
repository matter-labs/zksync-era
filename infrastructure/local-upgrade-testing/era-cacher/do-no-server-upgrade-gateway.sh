# era-cacher/reset.sh

# era-cacher/use-new-era.sh && cd zksync-working

upgrade_version="v28-1-vk"

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

zkstack dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage no-governance-prepare

zkstack dev run-ecosystem-upgrade  --upgrade-version $upgrade_version --ecosystem-upgrade-stage governance-stage0

zkstack  dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage governance-stage1

cd contracts/l1-contracts
UPGRADE_ECOSYSTEM_OUTPUT=script-out/v28-1-zk-os-upgrade-ecosystem.toml \
UPGRADE_ECOSYSTEM_OUTPUT_TRANSACTIONS=broadcast/EcosystemUpgrade_v28_1_zk_os.s.sol/9/run-latest.json  \
YAML_OUTPUT_FILE=script-out/v28-1-zk-os-local-output.yaml yarn upgrade-yaml-output-generator
cd ../../

zkstack dev run-chain-upgrade --upgrade-version $upgrade_version

zkstack  dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage governance-stage2

pkill -9 zksync_server
zkstack server --ignore-prerequisites --chain era &> ../rollup2.log &

sleep 10

zkstack dev test integration --no-deps --ignore-prerequisites --chain era
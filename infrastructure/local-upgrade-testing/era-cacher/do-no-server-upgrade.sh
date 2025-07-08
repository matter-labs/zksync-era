## era-cacher/reset.sh
#
## era-cacher/use-new-era.sh && cd zksync-working
#
#cargo install --path zkstack_cli/crates/zkstack --force --locked --features upgrades && zkstack dev clean containers && zkstack up --observability false
#zkstack dev contracts

zkstack ecosystem init --dev
# Server should be started in a different window for consistency
zkstack server --ignore-prerequisites --chain era &> ../rollup.log &
echo "Server started"

zkstack dev run-ecosystem-upgrade --upgrade-version v28-1-vk --ecosystem-upgrade-stage no-governance-prepare -v

zkstack dev run-ecosystem-upgrade  --upgrade-version v28-1-vk --ecosystem-upgrade-stage governance-stage0 -v

zkstack  dev run-ecosystem-upgrade --upgrade-version v28-1-vk --ecosystem-upgrade-stage governance-stage1
#
cd contracts/l1-contracts
UPGRADE_ECOSYSTEM_OUTPUT=script-out/v28-1-zk-os-upgrade-ecosystem.toml \
UPGRADE_ECOSYSTEM_OUTPUT_TRANSACTIONS=broadcast/EcosystemUpgrade_v28_1_zk_os.s.sol/9/run-latest.json  \
YAML_OUTPUT_FILE=script-out/v28-1-zk-os-local-output.yaml yarn upgrade-yaml-output-generator
cd ../../

zkstack dev run-chain-upgrade --upgrade-version v28-1-vk

zkstack  dev run-ecosystem-upgrade --upgrade-version v28-1-vk --ecosystem-upgrade-stage governance-stage2

pkill -9 zksync_server
zkstack server --ignore-prerequisites --chain era &> ../rollup2.log &

sleep 10

zkstack dev test integration --no-deps --ignore-prerequisites --chain era
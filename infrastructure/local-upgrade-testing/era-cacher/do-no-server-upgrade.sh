## era-cacher/reset.sh
#
## era-cacher/use-new-era.sh && cd zksync-working
#
zkstackup  --local --cargo-features upgrades
zkstack dev contracts

zkstack ecosystem init --dev
## Server should be started in a different window for consistency
zkstack server --ignore-prerequisites --chain era &> ../rollup.log &
echo "Server started"
##
zkstack dev run-ecosystem-upgrade --upgrade-version v28-1-vk --ecosystem-upgrade-stage no-governance-prepare -v

zkstack dev run-ecosystem-upgrade  --upgrade-version v28-1-vk --ecosystem-upgrade-stage governance-stage0 -v

zkstack  dev run-ecosystem-upgrade --upgrade-version v28-1-vk --ecosystem-upgrade-stage governance-stage1

zkstack dev run-chain-upgrade --upgrade-version v28-1-vk

zkstack  dev run-ecosystem-upgrade --upgrade-version v28-1-vk --ecosystem-upgrade-stage governance-stage2

pkill -9 zksync_server
zkstack server --ignore-prerequisites --chain era &> ../rollup2.log &

#sleep 10
#
#zkstack dev test integration --no-deps --ignore-prerequisites --chain era
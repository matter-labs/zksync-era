# Local upgrade testing

While it is theoretically possible to do it in CI-like style, it generally leads to needless recompilations, esp of rust
programs.

Here we contain the files/instructions needed to test an upgrade locally. The approach is to have two clones of the
repo, and to copy the config files between them. We could save target-etc in a different directory, but this process is
simpler and more robust.

We clone the two repos. We switch between them by copying them into zksync-working.

## Setup

```
mkdir upgrade-testing
cd upgrade-testing

git clone https://github.com/matter-labs/zksync-era.git zksync-old
git clone https://github.com/matter-labs/zksync-era.git zksync-new

cd zksync-old
git checkout main
git submodule update --init --recursive
cd ..

cd zksync-new
git checkout kl/reduced-interop-support
git submodule update --init --recursive
cd ..


cp -r zksync-new/infrastructure/local-gateway-upgrade-testing/era-cacher .

```

## Full flow

```
# need to reset the folders if the tests were run previously.
era-cacher/reset.sh

era-cacher/use-old-era.sh && cd zksync-working

cargo install --path zkstack_cli/crates/zkstack --force --locked  && zkstack dev clean all && zkstack up --observability false

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --verbose \
    --observability=false

## kl todo start chain here, turn it off.

cd .. && era-cacher/use-new-era.sh && cd zksync-working

cargo install --path zkstack_cli/crates/zkstack --force --locked
zkstack dev contracts
zkstack dev database migrate --prover false --core true

zkstack chain gateway-upgrade -- adapt-config

# Server should be started in a different window for consistency
zkstack server --ignore-prerequisites --chain era &> ./rollup.log &

zkstack e gateway-upgrade --ecosystem-upgrade-stage no-governance-prepare

zkstack e gateway-upgrade --ecosystem-upgrade-stage governance-stage1

zkstack chain gateway-upgrade -- prepare-stage1

# restart the server. wait for all L1 txs to exeucte!!!!
pkill -9 zksync_server
zkstack server --ignore-prerequisites --chain era &> ../rollup2.log &

zkstack chain gateway-upgrade -- schedule-stage1

# turn off the server => we need it because we need to somehow update validator timelock
# also getPriorityTreeStartIndex needs to be updated.

zkstack chain gateway-upgrade -- finalize-stage1

# restart the server

cd ~/zksync-era
zkstack dev test integration --no-deps --ignore-prerequisites --chain era
cd ~/zksync-era-current

zkstack ecosystem gateway-upgrade --ecosystem-upgrade-stage governance-stage2
zkstack ecosystem gateway-upgrade --ecosystem-upgrade-stage no-governance-stage2

# turn off the server

zkstack chain gateway-upgrade -- finalize-stage2

# turn on the server

zkstack dev test integration --no-deps --ignore-prerequisites --chain era



zkstack ecosystem gateway-upgrade --ecosystem-upgrade-stage governance-stage3
zkstack ecosystem gateway-upgrade --ecosystem-upgrade-stage no-governance-stage3

# in separate window
zkstack server --ignore-prerequisites --chain gateway

# wait for era server to finalize all L1 txs
# stop era server!

zkstack chain migrate-to-gateway --chain era --gateway-chain-name gateway

# restart era server!
zkstack dev test integration --no-deps --ignore-prerequisites --chain era
```

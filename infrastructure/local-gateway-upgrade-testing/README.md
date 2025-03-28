# Local upgrade testing

While it is theoretically possible to do it in CI-like style, it generally leads to needless recompilations, esp of rust
programs.

Here we contain the files/instructions needed to test the gateway upgrade locally.

## Step 0

- pull zksync-era to ~/zksync-era
- pull zksync-era-private to ~/zksync-era-private

## Step 1: Preparation

To easiest way to avoid needless is caching. There are two ways to avoid caching:

- Cache target/etc in a separate directory
- Have two folders of zksync-era and switch between those

We use the second approach for robustness and simplicity.

### Enabling `era-cacher`

Copy `era-cacher` to some other folder (as the zksync-era one will change) and add it to PATH, so it can be invoked.

You should download a clone of zksync-era, put it into the `zksync-era-old` directory. It should point to the commit of
`main` we will upgrade from.

## Step 2: spawning old chain

Run `use-old-era.sh`. The old contents of the zksync-era will be moved to `zksync-era-new` folder (there the gateway
version is stored), while the old one will be present in `zksync-era-new`.

## Step 3: Move to new chain and upgrade it

Use upgrade scripts as in the example below.

## Full flow

```
# make sure that there are 2 folders: zksync-era with old era and zksync-era-private with new era
# if test was run previously you probably need to move folder
mv ~/zksync-era-current ~/zksync-era-private

cd ~ && use-old-era.sh && cd ./zksync-era-current

zkstackup --local && zkstack dev clean all && zkstack up --observability false

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --verbose \
    --observability=false

cd ~ && use-new-era.sh && cd ./zksync-era-current

zkstackup --local
zkstack dev contracts
zkstack dev database migrate

zkstack chain gateway-upgrade -- adapt-config

# Server should be started in a different window for consistency
zkstack server --ignore-prerequisites --chain era

zkstack e gateway-upgrade --ecosystem-upgrade-stage no-governance-prepare

# only if chain has weth deployed before upgrade.
# i.e. you must run it iff `predeployed_l2_wrapped_base_token_address` is set in config.
zkstack chain gateway-upgrade -- set-l2weth-for-chain

zkstack e gateway-upgrade --ecosystem-upgrade-stage governance-stage1

zkstack chain gateway-upgrade -- prepare-stage1

# restart the server. wait for all L1 txs to exeucte!!!!

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

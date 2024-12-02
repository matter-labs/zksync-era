# Local upgrade testing


While it is theoretically possible to do it in CI-like style, it generally leads to needless recompilations, esp of rust programs.

Here we contain the files/instructions needed to test the gateway upgrade locally.

## Step 1: Preparation

To easiest way to avoid needless is caching. There are two ways to avoid caching:

- Cache target/etc in a separate directory
- Have two folders of zksync-era and switch between those

We use the second approach for robustness and simplicity.

### Enabling `era-cacher`

Copy `era-cacher` to some other folder (as the zksync-era one will change) and add it to PATH, so it can be invoked.

You should download a clone of zksync-era, put it into the `zksync-era-old` directory. It should point to the commit of `main` we will upgrade from.

## Step 2: spawning old chain

Run `use-old-era.sh`. The old contents of the zksync-era will be moved to `zksync-era-new` folder (there the gateway version is stored), while the old one will be present in `zksync-era-new`.

## Step 3: Move to new chain and upgrade it

Use upgrade scripts as in the example below.

## Full flow


```
use-old-era.sh 

zkt && zks clean all

zki up

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --verbose \
    --observability=false

use-new-era.sh 

zkstack dev contracts
zks database migrate
zki chain gateway-upgrade -- adapt-config
# remove nulls from the config

zk_inception server --ignore-prerequisites --chain era

zki e gateway-upgrade --ecosystem-upgrade-stage no-governance-prepare
zki e gateway-upgrade --ecosystem-upgrade-stage governance-stage1

zki chain gateway-upgrade -- prepare-stage1

# restart the server. wait for all txs to exeucte!!!!

zki chain gateway-upgrade -- schedule-stage1

# turn off the server => we need it because we need to somehow update validator timelock
# also getPriorityTreeStartIndex needs to be updated.

zki chain gateway-upgrade -- finalize-stage1

# restart the server

zk_supervisor test integration --no-deps --ignore-prerequisites --chain era

zki e gateway-upgrade --ecosystem-upgrade-stage governance-stage2
zki e gateway-upgrade --ecosystem-upgrade-stage no-governance-stage2

# turn off the server

zki chain gateway-upgrade -- finalize-stage2

# turn on the server

zk_supervisor test integration --no-deps --ignore-prerequisites --chain era
```


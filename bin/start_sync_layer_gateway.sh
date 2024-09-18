set -euox pipefail

zk
zk env dev
zk config compile
zk down
rm -rf "$ZKSYNC_HOME"db/*
rm -f gateway_server.log
mkdir -p "$ZKSYNC_HOME"volumes/postgres
mkdir -p "$ZKSYNC_HOME"volumes/reth/data
zk init -- --should-check-postgres false --skip-submodules-checkout

zk server &>gateway_server.log &
sleep 5

#Prepare the server to be the synclayer
zk dev2 supply-rich-wallets
zk contract build --zkSync
zk contract prepare-sync-layer
zk contract register-sync-layer-counterpart

sleep 360000;

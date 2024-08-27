set -euox pipefail


CHAIN_ID=$(<last_chain_id)
CHAIN_ID=$((CHAIN_ID + 1))
echo $CHAIN_ID > last_chain_id

DIFF=$((CHAIN_ID * 100))
CHAIN_ID=$((CHAIN_ID + 1000))

rm -f l3_server_pre_gateway.log

#Prepare launch sync-layer based chain
zk env dev
zk config prepare-l1-hyperchain --env-name test-chain --chain-id $CHAIN_ID
zk env test-chain
zk config compile test-chain --diff $DIFF
zk init hyper -- --skip-contract-compilation
zk server --time-to-live 40 &>l3_server_pre_gateway.log
sleep 40

export ETH_SENDER_SENDER_TX_AGGREGATION_PAUSED=true
zk server --time-to-live 20

zk contract migrate-to-sync-layer
zk contract prepare-sync-layer-validators
zk contract update-config-for-sync-layer

export ETH_SENDER_SENDER_TX_AGGREGATION_PAUSED=false
zk server

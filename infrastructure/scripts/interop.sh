#!/bin/bash

SECOND_CHAIN_NAME="${1:-validium}"
export SECOND_CHAIN_NAME

zkstack dev clean containers && zkstack up -o false
zkstack dev contracts

# zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
#     --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
#     --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
#     --server-db-name=zksync_server_localhost_era \
#     --ignore-prerequisites --observability=false \
#     --chain era \
#     --update-submodules false

zkstack dev generate-genesis

export PRIVATE_KEY=0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110

if [ "$SECOND_CHAIN_NAME" = "vbase_token" ]; then
  # === Deploy test token ===
  export BASE_TOKEN_ADDRESS=$(
    forge create ./contracts/l1-contracts/contracts/dev-contracts/TestnetERC20Token.sol:TestnetERC20Token \
      --private-key $PRIVATE_KEY \
      --broadcast \
      --gas-price 10000 \
      --constructor-args "TestToken" "TT" 18 |
      grep "Deployed to:" | awk '{print $3}'
  )
else
  export BASE_TOKEN_ADDRESS=0x0000000000000000000000000000000000000001
fi
echo "BASE_TOKEN_ADDRESS: $BASE_TOKEN_ADDRESS"

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --observability=false \
    --chain era \
    --update-submodules false
    # --pause-deposits \


zkstack chain create \
        --chain-name $SECOND_CHAIN_NAME \
        --chain-id 260 \
        --prover-mode no-proofs \
        --wallet-creation localhost \
        --l1-batch-commit-data-generator-mode validium \
        --base-token-address $BASE_TOKEN_ADDRESS \
        --base-token-price-nominator 1 \
        --base-token-price-denominator 1 \
        --set-as-default false \
        --evm-emulator false \
        --ignore-prerequisites

zkstack chain init \
            --deploy-paymaster \
            --l1-rpc-url=http://127.0.0.1:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_$SECOND_CHAIN_NAME \
            --chain $SECOND_CHAIN_NAME \
            --validium-type no-da \
            --skip-priority-txs \
            --pause-deposits

zkstack chain create \
        --chain-name gateway \
        --chain-id 506 \
        --prover-mode no-proofs \
        --wallet-creation localhost \
        --l1-batch-commit-data-generator-mode rollup \
        --base-token-address 0x0000000000000000000000000000000000000001 \
        --base-token-price-nominator 1 \
        --base-token-price-denominator 1 \
        --set-as-default false \
        --evm-emulator false \
        --ignore-prerequisites

zkstack chain init \
            --deploy-paymaster \
            --l1-rpc-url=http://127.0.0.1:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_gateway \
            --chain gateway

zkstack server --ignore-prerequisites --chain era &> ./zruns/era1.log &
zkstack server wait --ignore-prerequisites --verbose --chain era

sh ./infrastructure/scripts/bridge_eth_to_era.sh era
sh ./infrastructure/scripts/bridge_token_to_era.sh era

sh ./infrastructure/scripts/bridge_token_from_era.sh era
sleep 30

pkill -9 zksync_server
sleep 30

# zkstack server --ignore-prerequisites --chain validium &> ./zruns/validium1.log &
# zkstack server wait --ignore-prerequisites --verbose --chain validium
# # we need to fund the address before migration. todo enable base token transfers.
# sh ./infrastructure/scripts/bridge_eth_to_era.sh validium
# sleep 30

# pkill -9 zksync_server
# sleep 10

zkstack chain gateway create-tx-filterer --chain gateway --ignore-prerequisites
zkstack chain gateway convert-to-gateway --chain gateway --ignore-prerequisites
zkstack dev config-writer --path etc/env/file_based/overrides/tests/gateway.yaml --chain gateway
zkstack server --ignore-prerequisites --chain gateway &> ./zruns/gateway.log &


zkstack server wait --ignore-prerequisites --verbose --chain gateway

sleep 20

zkstack chain pause-deposits --chain era
zkstack chain gateway migrate-to-gateway --chain era --gateway-chain-name gateway
zkstack chain gateway finalize-chain-migration-to-gateway --chain era --gateway-chain-name gateway --deploy-paymaster
zkstack chain gateway migrate-to-gateway --chain $SECOND_CHAIN_NAME --gateway-chain-name gateway
zkstack chain gateway finalize-chain-migration-to-gateway --chain $SECOND_CHAIN_NAME --gateway-chain-name gateway --deploy-paymaster

# Chain registrations on chain `validium` were skipped, as its deposits were paused. Do these registrations now
zkstack chain --chain $SECOND_CHAIN_NAME register-on-all-chains 

zkstack server --ignore-prerequisites --chain era &> ./zruns/era.log & 
zkstack server --ignore-prerequisites --chain $SECOND_CHAIN_NAME &> ./zruns/$SECOND_CHAIN_NAME.log & 
zkstack server wait --ignore-prerequisites --verbose --chain era
zkstack server wait --ignore-prerequisites --verbose --chain $SECOND_CHAIN_NAME

zkstack chain gateway migrate-token-balances --to-gateway --chain era --gateway-chain-name gateway
zkstack chain gateway migrate-token-balances --to-gateway --chain $SECOND_CHAIN_NAME --gateway-chain-name gateway


zkstack dev init-test-wallet --chain era
zkstack dev init-test-wallet --chain $SECOND_CHAIN_NAME
# Runs interop integration test between era-validium in parallel
mkdir -p zlogs
# zkstack dev test integration -t "Interop"  --verbose --chain era --no-deps --second-chain $SECOND_CHAIN_NAME &> zlogs/era.logs
# ./bin/run_on_all_chains.sh "zkstack dev test integration -t 'Interop' --verbose" \
#             "era,validium" zlogs/ \
#             'era:--evm' 'validium:--evm'

#!/bin/bash

# zkstackup -g --local

zkstack dev clean containers && zkstack up -o false
zkstack dev contracts

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --observability=false \
    --chain era \
    --update-submodules false

# zkstack server

zkstack dev generate-genesis

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --observability=false \
    --chain era \
    --update-submodules false

zkstack chain create \
        --chain-name validium \
        --chain-id 260 \
        --prover-mode no-proofs \
        --wallet-creation localhost \
        --l1-batch-commit-data-generator-mode validium \
        --base-token-address 0x0000000000000000000000000000000000000001 \
        --base-token-price-nominator 1 \
        --base-token-price-denominator 1 \
        --set-as-default false \
        --evm-emulator false \
        --ignore-prerequisites --update-submodules false 

zkstack chain init \
            --deploy-paymaster \
            --l1-rpc-url=http://127.0.0.1:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_validium \
            --chain validium --update-submodules false \
            --validium-type no-da

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
        --ignore-prerequisites --update-submodules false 

zkstack chain init \
            --deploy-paymaster \
            --l1-rpc-url=http://127.0.0.1:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_gateway \
            --chain gateway --update-submodules false


zkstack chain gateway convert-to-gateway --chain gateway --ignore-prerequisites
zkstack dev config-writer --path etc/env/file_based/overrides/tests/gateway.yaml --chain gateway
zkstack server --ignore-prerequisites --chain gateway &> ./zruns/gateway.log &


zkstack server wait --ignore-prerequisites --verbose --chain gateway

sleep 10

zkstack chain gateway migrate-to-gateway --chain era --gateway-chain-name gateway
zkstack chain gateway migrate-to-gateway --chain validium --gateway-chain-name gateway

zkstack server --ignore-prerequisites --chain era &> ./zruns/era.log &

zkstack server wait --ignore-prerequisites --verbose --chain era
zkstack server --ignore-prerequisites --chain validium &> ./zruns/validium.log &

zkstack server wait --ignore-prerequisites --verbose --chain validium

# Runs interop integration test between era-validium in parallel
mkdir -p zlogs
./bin/run_on_all_chains.sh "zkstack dev test integration -t 'L1 ERC20' --verbose" \
            "era,validium" zlogs/ \
            'era:--evm' 'validium:--evm'

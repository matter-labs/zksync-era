#!/bin/bash

# Ecosystem init
zkstack dev clean containers && zkstack up -o false
zkstack dev contracts

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --observability=false \
    --chain era \
    --update-submodules=false

zkstack dev generate-genesis

# Create and initialize gateway chain
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
    --ignore-prerequisites \
    --evm-emulator false

zkstack chain init \
    --deploy-paymaster \
    --l1-rpc-url=http://localhost:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_gateway \
    --chain gateway \
    --validium-type no-da

zkstack chain gateway create-tx-filterer --chain gateway --ignore-prerequisites
zkstack chain gateway convert-to-gateway --chain gateway --ignore-prerequisites

# Ensure the gateway server is running, as it gets killed after the tests finish
zkstack server --ignore-prerequisites --chain gateway &> ./gateway.log & 
zkstack server wait --ignore-prerequisites --verbose --chain gateway

# Run token balance migration tests
export CI=1 # Needed to avoid killing the server in core/tests/highlevel-test-tools/global-setup.ts#48
export USE_GATEWAY_CHAIN=WITH_GATEWAY
export TESTED_CHAIN_TYPE=era
yarn --cwd core/tests/highlevel-test-tools test -t "Token balance migration tests"

#!/bin/bash
# to use this properly comment out the section that fails the node if the bootloader has is wrong in genesis/src/lib.rs


rm -rf chains/second
zkstack dev clean containers && zkstack up -o false
zkstack dev contracts

zkstack dev generate-genesis

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --observability=false \
    --chain era \
    --update-submodules false

zkstack chain create \
        --chain-name second \
        --chain-id 505 \
        --prover-mode no-proofs \
        --wallet-creation localhost \
        --l1-batch-commit-data-generator-mode rollup \
        --base-token-address 0x0000000000000000000000000000000000000001 \
        --base-token-price-nominator 1 \
        --base-token-price-denominator 1 \
        --set-as-default false \
        --evm-emulator false \
        --ignore-prerequisites --update-submodules false 
        # --skip-contract-compilation-override

zkstack chain init \
            --deploy-paymaster \
            --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_gateway \
            --chain second --update-submodules false
        #     --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
        #     --prover-db-name=zksync_prover_localhost_gateway \

 zkstack server --ignore-prerequisites --chain era &> ./rollup.log &

 zkstack server --ignore-prerequisites --chain second &> ./second.log &

# sleep 20

# zkstack dev test integration -t "Interop" --no-deps --ignore-prerequisites --chain era

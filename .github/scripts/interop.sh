sudo rm -rf ./volumes && zkstack dev clean containers && zkstack up -o false

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
            --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_era \
            --ignore-prerequisites --observability=false --skip-submodules-checkout \
            --chain era # --no-port-reallocation
        #     --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
        #     --prover-db-name=zksync_prover_localhost_era \
            # --skip-contract-compilation-override \

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
        --ignore-prerequisites --skip-submodules-checkout --skip-contract-compilation-override

zkstack chain init \
            --deploy-paymaster \
            --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_gateway \
            --chain second --skip-submodules-checkout --no-port-reallocation
        #     --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
        #     --prover-db-name=zksync_prover_localhost_gateway \

# zkstack server --ignore-prerequisites --chain era &> ./rollup.log & 

zkstack server --ignore-prerequisites --chain second &> ./second.log & 

# sleep 20

# zkstack dev test integration -t "Interop" --no-deps --ignore-prerequisites --chain era

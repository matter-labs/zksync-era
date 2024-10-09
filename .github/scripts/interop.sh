sudo rm -rf ./volumes && zk_supervisor clean containers && zk_inception up -o false

zk_inception ecosystem init --deploy-paymaster --deploy-erc20 \
            --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_era \
            --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --prover-db-name=zksync_prover_localhost_era \
            --ignore-prerequisites --observability=false --skip-submodules-checkout \
            --chain era \
            # --skip-contract-compilation-override \

zk_inception chain create \
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

zk_inception chain init \
            --deploy-paymaster \
            --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_gateway \
            --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --prover-db-name=zksync_prover_localhost_gateway \
            --chain second --skip-submodules-checkout --no-port-reallocation

zk_inception server --ignore-prerequisites --chain era &> ./rollup.log & 

zk_inception server --ignore-prerequisites --chain second &> ./second.log & 

# sleep 20

# zk_supervisor test integration -t "Interop" --no-deps --ignore-prerequisites --chain era

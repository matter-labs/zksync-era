sudo rm -rf ./volumes && zk_supervisor clean containers && zk_inception up -o false

zk_inception ecosystem init --deploy-paymaster --deploy-erc20 \
            --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_era \
            --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --prover-db-name=zksync_prover_localhost_era \
            --ignore-prerequisites --observability=false --skip-submodules-checkout \
            --chain era
        #     --skip-contract-compilation-override \
            # --verbose \

zk_inception chain create \
        --chain-name gateway \
        --chain-id 506 \
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
            --port-offset 4000 \
            --chain gateway --skip-submodules-checkout

zk_inception chain convert-to-gateway --chain gateway --ignore-prerequisites

zk_inception server --ignore-prerequisites --chain gateway &> ./gateway.log & 

sleep 20

zk_inception chain migrate-to-gateway --chain era --gateway-chain-name gateway 

zk_inception chain migrate-from-gateway --chain era --gateway-chain-name gateway

zk_inception chain migrate-to-gateway --chain era --gateway-chain-name gateway

zk_inception server --ignore-prerequisites --chain era &> ./rollup.log &

sleep 20

zk_supervisor test integration --no-deps --ignore-prerequisites --chain era
sudo rm -rf ./volumes && zk_supervisor clean containers && zk_inception up -o false

zk_inception ecosystem init --deploy-paymaster --deploy-erc20 \
            --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_era \
            --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --prover-db-name=zksync_prover_localhost_era \
            --ignore-prerequisites --observability=false --skip-submodules-checkout \
            --chain era \
            --verbose
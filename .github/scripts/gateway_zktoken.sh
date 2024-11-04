sudo rm -rf ./volumes && zk_supervisor clean containers && zkstack up -o false

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
            --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_era \
            --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --prover-db-name=zksync_prover_localhost_era \
            --ignore-prerequisites --observability=false --skip-submodules-checkout \
            --chain era \
            --verbose

zkstack server --ignore-prerequisites --chain era &> ./era.log & 

sleep 20

zkstack chain deploy-and-bridge-zk --chain era --verbose     

# Define the path to the TOML file
TOML_FILE="contracts/l1-contracts/script-out/output-deploy-zk-token.toml"

# Extract the l1Address from the TOML file
zkL1Address=$(grep -A 1 "\[ZK.l1Address\]" "$TOML_FILE" | grep "l1Address" | awk -F' = ' '{print $2}' | tr -d '"' | tr -d ' ' | tr -d '\n')

# Check if the address starts with 0x and remove it
if [[ $zkL1Address == 0x* ]]; then
    zkL1Address="${zkL1Address:2}"
fi

# Output the extracted and sliced l1Address (for debugging purposes)
echo "Sliced l1Address: $zkL1Address"
  
zkstack chain create \
        --chain-name gateway \
        --chain-id 505 \
        --prover-mode no-proofs \
        --wallet-creation localhost \
        --l1-batch-commit-data-generator-mode rollup \
        --base-token-address $zkL1Address \
        --base-token-price-nominator 1 \
        --base-token-price-denominator 1 \
        --set-as-default false \
        --ignore-prerequisites --skip-submodules-checkout --skip-contract-compilation-override

zkstack chain init \
            --deploy-paymaster \
            --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_gateway \
            --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --prover-db-name=zksync_prover_localhost_gateway \
            --chain gateway --skip-submodules-checkout

zkstack chain convert-to-gateway --chain gateway --ignore-prerequisites

SERVER_PID=$(lsof -t -i :3075)

kill -2 $SERVER_PID

zkstack server --ignore-prerequisites --chain gateway &> ./gateway.log & 

sleep 20

zkstack chain migrate-to-gateway --chain era --gateway-chain-name gateway 

zkstack chain migrate-from-gateway --chain era --gateway-chain-name gateway

zkstack chain migrate-to-gateway --chain era --gateway-chain-name gateway

zkstack server --ignore-prerequisites --chain era &> ./rollup.log &

sleep 20

zk_supervisor test integration --no-deps --ignore-prerequisites --chain era
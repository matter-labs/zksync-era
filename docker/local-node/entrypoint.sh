#!/bin/bash
set -ea
# RUST VERSION + CLANG??
# dockerd --host=tcp://0.0.0.0:2375 --host=unix:///var/run/docker.sock > /var/log/dockerd.log 2>&1 &

apt update && apt install -y clang llvm-dev libclang-dev
# find /usr -name "libclang.so*"
export LIBCLANG_PATH=/usr/lib/llvm-14/lib
# apt install -y pkg-config libssl-dev

./zkstack_cli/zkstackup/install -g --path ./zkstack_cli/zkstackup/zkstackup

zkstackup -g --local --cargo-features gateway

# apt-get update && apt-get install -y docker.io
# curl -L "https://github.com/docker/compose/releases/download/v2.21.0/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose
# chmod +x /usr/local/bin/docker-compose
apt-get update && apt-get install -y ca-certificates curl gnupg
mkdir -m 0755 /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian \
  $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null
apt-get update
apt-get install -y docker-compose-plugin
curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash
export PATH=$HOME/.foundry/bin:$PATH

# nohup dockerd --host=tcp://0.0.0.0:2375 --host=unix:///var/run/docker.sock > /var/log/dockerd.log 2>&1 &

# zkstack dev contracts

# export COMPOSE_PROJECT_NAME=my_project_name
# export GITHUB_WORKSPACE=my_github_workspace

# zkstack up -o false

# docker cp /chaindata/reth_config my_project_name-reth-1:/chaindata/reth_config

# socat TCP-LISTEN:5432,fork TCP:host.docker.internal:5432 &
# socat TCP-LISTEN:8545,fork TCP:host.docker.internal:8545 &

# docker run --privileged -it docker:dind

# DATABASE_PROVER_URL=postgres://postgres:notsecurepassword@localhost/prover_local
# DATABASE_URL=postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era 
# DATABASE_URL=postgres://postgres:notsecurepassword@localhost/zksync_local
# DATABASE_URL=postgres://postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --verbose \
    --observability=false \
    --update-submodules=false

# CUSTOM_TOKEN_ADDRESS=$(awk -F": " '/tokens:/ {found_tokens=1} found_tokens && /DAI:/ {found_dai=1} found_dai && /address:/ {print $2; exit}' ./configs/erc20.yaml)
# echo "CUSTOM_TOKEN_ADDRESS=$CUSTOM_TOKEN_ADDRESS"
# echo "CUSTOM_TOKEN_ADDRESS=$CUSTOM_TOKEN_ADDRESS" >> $GITHUB_ENV

# zkstack chain create \
#     --chain-name validium \
#     --chain-id sequential \
#     --prover-mode no-proofs \
#     --wallet-creation localhost \
#     --l1-batch-commit-data-generator-mode validium \
#     --base-token-address 0x0000000000000000000000000000000000000001 \
#     --base-token-price-nominator 1 \
#     --base-token-price-denominator 1 \
#     --set-as-default false \
#     --ignore-prerequisites \
#     --evm-emulator false

# zkstack chain init \
#     --deploy-paymaster \
#     --l1-rpc-url=http://localhost:8545 \
#     --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
#     --server-db-name=zksync_server_localhost_validium \
#     --chain validium \
#     --validium-type no-da

# zkstack chain create \
#     --chain-name custom_token \
#     --chain-id sequential \
#     --prover-mode no-proofs \
#     --wallet-creation localhost \
#     --l1-batch-commit-data-generator-mode rollup \
#     --base-token-address ${{ env.CUSTOM_TOKEN_ADDRESS }} \
#     --base-token-price-nominator 314 \
#     --base-token-price-denominator 1000 \
#     --set-as-default false \
#     --ignore-prerequisites \
#     --evm-emulator false

# zkstack chain init \
#       --deploy-paymaster \
#       --l1-rpc-url=http://localhost:8545 \
#       --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
#       --server-db-name=zksync_server_localhost_custom_token \
#       --chain custom_token \
#       --validium-type no-da

# zkstack chain create \
#     --chain-name offline_chain \
#     --chain-id sequential \
#     --prover-mode no-proofs \
#     --wallet-creation localhost \
#     --l1-batch-commit-data-generator-mode rollup \
#     --base-token-address 0x0000000000000000000000000000000000000001 \
#     --base-token-price-nominator 1 \
#     --base-token-price-denominator 1 \
#     --set-as-default false \
#     --ignore-prerequisites \
#     --evm-emulator false

# zkstack chain build-transactions --chain offline_chain --l1-rpc-url http://127.0.0.1:8545

# governor_pk=$(awk '/governor:/ {flag=1} flag && /private_key:/ {print $2; exit}' ./configs/wallets.yaml)

# zkstack dev send-transactions \
#     --file ./transactions/chain/offline_chain/register-zk-chain-txns.json \
#     --l1-rpc-url http://127.0.0.1:8545 \
#     --private-key $governor_pk

# bridge_hub=$(awk '/bridgehub_proxy_addr/ {print $2}' ./configs/contracts.yaml)
# chain_id=$(awk '/chain_id:/ {print $2}' ./chains/offline_chain/ZkStack.yaml)

# hyperchain_output=$(ci_run cast call $bridge_hub "getHyperchain(uint256)" $chain_id)

# if [[ $hyperchain_output == 0x* && ${#hyperchain_output} -eq 66 ]]; then
#     echo "Chain successfully registered: $hyperchain_output"
# else
#     echo "Failed to register chain: $hyperchain_output"
#     exit 1
# fi

# zkstack chain create \
#     --chain-name consensus \
#     --chain-id sequential \
#     --prover-mode no-proofs \
#     --wallet-creation localhost \
#     --l1-batch-commit-data-generator-mode validium \
#     --base-token-address ${{ env.CUSTOM_TOKEN_ADDRESS }} \
#     --base-token-price-nominator 314 \
#     --base-token-price-denominator 1000 \
#     --set-as-default false \
#     --ignore-prerequisites \
#     --evm-emulator false

# zkstack chain init \
#     --deploy-paymaster \
#     --l1-rpc-url=http://localhost:8545 \
#     --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
#     --server-db-name=zksync_server_localhost_consensus \
#     --chain consensus \
#     --validium-type no-da

# CHAINS="era,validium,custom_token,consensus"
# echo "CHAINS=$CHAINS"

# zkstack chain create \
#     --chain-name gateway \
#     --chain-id 505 \
#     --prover-mode no-proofs \
#     --wallet-creation localhost \
#     --l1-batch-commit-data-generator-mode rollup \
#     --base-token-address 0x0000000000000000000000000000000000000001 \
#     --base-token-price-nominator 1 \
#     --base-token-price-denominator 1 \
#     --set-as-default false \
#     --ignore-prerequisites \
#     --evm-emulator false
          
# zkstack chain init \
#     --deploy-paymaster \
#     --l1-rpc-url=http://localhost:8545 \
#     --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
#     --server-db-name=zksync_server_localhost_gateway \
#     --chain gateway \
#     --validium-type no-da
          
# zkstack chain convert-to-gateway --chain gateway --ignore-prerequisites

# zkstack server --ignore-prerequisites --chain gateway &> ${{ env.SERVER_LOGS_DIR }}/gateway.log &
# zkstack server wait --ignore-prerequisites --verbose --chain gateway

# zkstack chain migrate-to-gateway --chain era --gateway-chain-name gateway
# zkstack chain migrate-to-gateway --chain validium --gateway-chain-name gateway
# zkstack chain migrate-to-gateway --chain custom_token --gateway-chain-name gateway
# zkstack chain migrate-to-gateway --chain consensus --gateway-chain-name gateway
          
# zkstack server --ignore-prerequisites --chain era &> ${{ env.SERVER_LOGS_DIR }}/rollup.log &
# zkstack server --ignore-prerequisites --chain validium &> ${{ env.SERVER_LOGS_DIR }}/validium.log &
# zkstack server --ignore-prerequisites --chain custom_token &> ${{ env.SERVER_LOGS_DIR }}/custom_token.log &
# zkstack server --ignore-prerequisites --chain consensus \
#             --components=api,tree,eth,state_keeper,housekeeper,commitment_generator,vm_runner_protective_reads,vm_runner_bwip,vm_playground,da_dispatcher,consensus \
#             &> ${{ env.SERVER_LOGS_DIR }}/consensus.log &

# zkstack server wait --ignore-prerequisites --verbose --chain era
# zkstack server wait --ignore-prerequisites --verbose --chain validium
# zkstack server wait --ignore-prerequisites --verbose --chain custom_token
# zkstack server wait --ignore-prerequisites --verbose --chain consensus

# start server
zkstack server

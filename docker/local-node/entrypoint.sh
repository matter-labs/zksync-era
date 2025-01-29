#!/bin/bash
# set -ea
# RUST VERSION + CLANG??
# dockerd --host=tcp://0.0.0.0:2375 --host=unix:///var/run/docker.sock > /var/log/dockerd.log 2>&1 &

# apt update && apt install -y clang llvm-dev libclang-dev
# find /usr -name "libclang.so*"
# export LIBCLANG_PATH=/usr/lib/llvm-14/lib
# apt install -y pkg-config libssl-dev

# ./zkstack_cli/zkstackup/install -g --path ./zkstack_cli/zkstackup/zkstackup

# zkstackup -g --local --cargo-features gateway

# apt-get update && apt-get install -y docker.io
# curl -L "https://github.com/docker/compose/releases/download/v2.21.0/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose
# chmod +x /usr/local/bin/docker-compose
# apt-get update && apt-get install -y ca-certificates curl gnupg
# # mkdir -m 0755 /etc/apt/keyrings
# curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
# echo \
#   "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian \
#   $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null
# apt-get update
# apt-get install -y docker-compose-plugin
# curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash
# export PATH=$HOME/.foundry/bin:$PATH

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


# These 3 env variables must be provided.
if [ -z "$DATABASE_URL" ]; then
  echo "ERROR: DATABASE_URL is not set."
  exit 1
fi

if [ -z "$DATABASE_PROVER_URL" ]; then
  echo "ERROR: DATABASE_PROVER_URL is not set."
  exit 1
fi

if [ -z "$ETH_CLIENT_WEB3_URL" ]; then
  echo "ERROR: ETH_CLIENT_WEB3_URL is not set."
  exit 1
fi

# Function to update a key in a YAML file
update_config() {
  local file="$1"
  local key="$2"
  local new_value="$3"

  # Escape special characters for sed
  local escaped_key=$(echo "$key" | sed 's/\./\\./g')
  local pattern="^\\s*${escaped_key}:.*$"

  # Check if the key exists in the file
  if grep -qE "$pattern" "$file"; then
    # Update the existing key
    sed -i "s|$pattern|${key}: $new_value|" "$file"
    echo "Updated '$key' in $file."
  else
    # Append the key if it doesn't exist
    echo "$key: $new_value" >> "$file"
    echo "Added '$key' to $file."
  fi
}


# wait till db service is ready
until psql ${DATABASE_URL%/*} -c '\q'; do
  echo >&2 "Postgres is unavailable - sleeping"
  sleep 5
done

echo "Initialing local environment"

update_config "/chains/era/configs/secrets.yaml" "server_url" "$DATABASE_URL"
update_config "/chains/era/configs/secrets.yaml" "prover_url" "$DATABASE_PROVER_URL"
update_config "/chains/era/configs/secrets.yaml" "l1_rpc_url" "$ETH_CLIENT_WEB3_URL"

# Extract the database name (everything after the last '/')
SERVER_DB_NAME="${DATABASE_URL##*/}"

# Extract the database URL without the database name
SERVER_DB_URL="${DATABASE_URL%/*}"

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=$ETH_CLIENT_WEB3_URL \
    --server-db-url="$SERVER_DB_URL" \
    --server-db-name="$SERVER_DB_NAME" \
    --ignore-prerequisites --verbose \
    --observability=false \
    --update-submodules=false

# if [ -z "$MASTER_URL" ]; then
#   echo "Running as zksync master"
# else
#   # If running in slave mode - wait for the master to be up and running.
#   echo "Waiting for zksync master to init hyperchain"
#   until curl --fail ${MASTER_HEALTH_URL}; do
#     echo >&2 "Master zksync not ready yet, sleeping"
#     sleep 5
#   done
# fi

# if [ -n "$LEGACY_BRIDGE_TESTING" ]; then
#   if [ -z "$MASTER_URL" ]; then
#     echo "Running in legacy bridge testing mode"
#   else
#     # LEGACY_BRIDGE_TESTING flag is for the master only
#     unset LEGACY_BRIDGE_TESTING
#   fi
# fi

# Normally, the /etc/env and /var/lib/zksync/data should be mapped to volumes
# so that they are persisted between docker restarts - which would allow even faster starts.

# We use the existance of this init file to decide whether to restart or not.
# INIT_FILE="/var/lib/zksync/data/INIT_COMPLETED.remove_to_reset"

# if [ -f "$INIT_FILE" ]; then
#   echo "Initialization was done in the past - simply starting server"
# else
#   echo "Initialing local environment"

#   mkdir -p /var/lib/zksync/data

#   update_config "/etc/env/base/private.toml" "database_url" "$DATABASE_URL"
#   update_config "/etc/env/base/private.toml" "database_prover_url" "$DATABASE_PROVER_URL"
#   update_config "/etc/env/base/eth_client.toml" "web3_url" "$ETH_CLIENT_WEB3_URL"
#   # Put database in a special /var/lib directory so that it is persisted between docker runs.
#   update_config "/etc/env/base/database.toml" "path" "/var/lib/zksync/data"
#   update_config "/etc/env/base/database.toml" "state_keeper_db_path" "/var/lib/zksync/data/state_keeper"
#   update_config "/etc/env/base/database.toml" "backup_path" "/var/lib/zksync/data/backups"

  # if [ -z "$MASTER_URL" ]; then
  #   echo "Starting with hyperchain"
  # else
  #   # Updates all the stuff (from the '/etc/master_env') - it assumes that it is mapped via docker compose.
  #   zk f yarn --cwd /infrastructure/local-setup-preparation join
  # fi

  # Perform initialization (things needed to be done only if you're running in the master mode)
  # if [ -z "$MASTER_URL" ]; then
  #   zk contract deploy-verifier
  #   zk run deploy-erc20 dev # (created etc/tokens/localhost)

  #   ## init bridgehub state transition
  #   zk contract deploy # (deploy L1)
  #   zk contract initialize-governance
  #   zk contract initialize-validator
  # fi


  # if [ -z "$CUSTOM_BASE_TOKEN" ]; then
  #   echo "Starting chain with ETH as gas token"

  #   if [ -z "$VALIDIUM_MODE" ]; then
  #     ## init hyperchain in rollup mode
  #     zk contract register-hyperchain
  #   else
  #     zk contract register-hyperchain --deployment-mode 1
  #   fi
  # else
  #   echo "Starting chain with custom gas token $CUSTOM_BASE_TOKEN"
  #   zk contract register-hyperchain --base-token-name $CUSTOM_BASE_TOKEN
  # fi

# fi


# start server
zkstack server

#!/bin/bash
set -ea

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

# Default chain name if not set
CHAIN_NAME=${CHAIN_NAME:-custom_token}

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

# Determine the correct config path based on MASTER_URL
if [ -z "$MASTER_URL" ]; then
  CONFIG_PATH="/chains/era/configs"
  echo "Updating configuration for MASTER chain..."
else
  CONFIG_PATH="/chains/${CHAIN_NAME}/configs"
  echo "Updating configuration for ${CHAIN_NAME} chain..."
fi

echo "Configuration updated successfully."

# Extract the database name (everything after the last '/')
SERVER_DB_NAME="${DATABASE_URL##*/}"

# Extract the database URL without the database name
SERVER_DB_URL="${DATABASE_URL%/*}"

GATEWAY_SERVER_DB_URL="zksync_server_localhost_gateway"

if [ -z "$MASTER_URL" ]; then
  echo "Running as zksync master"

  zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=$ETH_CLIENT_WEB3_URL \
    --server-db-url="$SERVER_DB_URL" \
    --server-db-name="$SERVER_DB_NAME" \
    --ignore-prerequisites --verbose \
    --observability=false \
    --skip-contract-compilation-override \
    --update-submodules=false \
    --server-command /zksync_server

  if [ "$GATEWAY" = "true" ]; then
    echo "Setting up gateway chain"

    zkstack chain create \
      --chain-name gateway \
      --chain-id 505 \
      --prover-mode no-proofs \
      --wallet-creation localhost \
      --l1-batch-commit-data-generator-mode rollup \
      --base-token-address 0x0000000000000000000000000000000000000001 \
      --base-token-price-nominator 1 \
      --base-token-price-denominator 1 \
      --set-as-default false \
      --ignore-prerequisites \
      --update-submodules=false \
      --evm-emulator false
    
    zkstack chain init \
      --deploy-paymaster \
      --l1-rpc-url=$ETH_CLIENT_WEB3_URL \
      --server-db-url="$SERVER_DB_URL" \
      --server-db-name="$GATEWAY_SERVER_DB_URL" \
      --chain gateway \
      --update-submodules=false \
      --server-command /zksync_server \
      --validium-type no-da

    zkstack chain convert-to-gateway --chain gateway --ignore-prerequisites
  fi
  
  rm -rf /usr/local/share/.cache /contracts/node_modules /node_modules 

  if [ "$GATEWAY" = "true" ]; then
    echo "Migrating era chain to gateway and starting servers..."
    
    mkdir server_logs

    # start server gateway
    /zksync_server --genesis-path ./chains/gateway/configs/genesis.yaml \
      --wallets-path ./chains/gateway/configs/wallets.yaml \
      --config-path ./chains/gateway/configs/general.yaml \
      --secrets-path ./chains/gateway/configs/secrets.yaml \
      --contracts-config-path ./chains/gateway/configs/contracts.yaml \
      &>server_logs/gateway.log &

    zkstack chain migrate-to-gateway --chain era --gateway-chain-name gateway

    # start era server
    /zksync_server --genesis-path ./chains/era/configs/genesis.yaml \
      --wallets-path ./chains/era/configs/wallets.yaml \
      --config-path ./chains/era/configs/general.yaml \
      --secrets-path ./chains/era/configs/secrets.yaml \
      --contracts-config-path ./chains/era/configs/contracts.yaml \
      --gateway-contracts-config-path ./chains/era/configs/gateway_chain.yaml
  else
    echo "Skipping gateway conversion."

    # start era server
    /zksync_server --genesis-path ./chains/era/configs/genesis.yaml \
      --wallets-path ./chains/era/configs/wallets.yaml \
      --config-path ./chains/era/configs/general.yaml \
      --secrets-path ./chains/era/configs/secrets.yaml \
      --contracts-config-path ./chains/era/configs/contracts.yaml
  fi
else
  zkstack chain create \
    --chain-name ${CHAIN_NAME} \
    --chain-id sequential \
    --prover-mode no-proofs \
    --wallet-creation localhost \
    --l1-batch-commit-data-generator-mode rollup \
    --base-token-address ${CUSTOM_TOKEN_ADDRESS} \
    --base-token-price-nominator 314 \
    --base-token-price-denominator 1000 \
    --set-as-default false \
    --ignore-prerequisites \
    --evm-emulator false \
    --update-submodules=false
  
  zkstack chain init \
    --deploy-paymaster \
    --l1-rpc-url=$ETH_CLIENT_WEB3_URL \
    --server-db-url="$SERVER_DB_URL" \
    --server-db-name="$SERVER_DB_NAME" \
    --chain ${CHAIN_NAME} \
    --validium-type no-da \
    --update-submodules=false \
    --server-command /zksync_server
  
  if [ "$GATEWAY" = "true" ]; then
    echo "Migrating custom base token chain on top of gateway chain"
    find ./chains -type f -exec sed -i 's|http://127.0.0.1:3150|'"${GATEWAY_URL}"'|g' {} +
    zkstack chain migrate-to-gateway --chain ${CHAIN_NAME} --gateway-chain-name gateway
  fi
  
  rm -rf /usr/local/share/.cache /contracts/node_modules /node_modules

  # If running in slave mode - wait for the master to be up and running.
  echo "Waiting for zksync master to init hyperchain"
  until curl --fail ${MASTER_HEALTH_URL}; do
    echo >&2 "Master zksync not ready yet, sleeping"
    sleep 5
  done

  if [ "$GATEWAY" = "true" ]; then
     # start server
    /zksync_server --genesis-path /chains/${CHAIN_NAME}/configs/genesis.yaml \
      --wallets-path /chains/${CHAIN_NAME}/configs/wallets.yaml \
      --config-path /chains/${CHAIN_NAME}/configs/general.yaml \
      --secrets-path /chains/${CHAIN_NAME}/configs/secrets.yaml \
      --contracts-config-path /chains/${CHAIN_NAME}/configs/contracts.yaml \
      --gateway-contracts-config-path ./chains/era/configs/gateway_chain.yaml
  else
     # start server
    /zksync_server --genesis-path /chains/${CHAIN_NAME}/configs/genesis.yaml \
      --wallets-path /chains/${CHAIN_NAME}/configs/wallets.yaml \
      --config-path /chains/${CHAIN_NAME}/configs/general.yaml \
      --secrets-path /chains/${CHAIN_NAME}/configs/secrets.yaml \
      --contracts-config-path /chains/${CHAIN_NAME}/configs/contracts.yaml
  fi
fi

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
  CONFIG_PATH="/chains/custom_token/configs"
  echo "Updating configuration for Custom Token chain..."
fi

echo "Configuration updated successfully."

# Extract the database name (everything after the last '/')
SERVER_DB_NAME="${DATABASE_URL##*/}"

# Extract the database URL without the database name
SERVER_DB_URL="${DATABASE_URL%/*}"

if [ -z "$MASTER_URL" ]; then
  echo "Running as zksync master"

  zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=$ETH_CLIENT_WEB3_URL \
    --server-db-url="$SERVER_DB_URL" \
    --server-db-name="$SERVER_DB_NAME" \
    --ignore-prerequisites --verbose \
    --observability=false \
    --update-submodules=false

  rm -rf /usr/local/share/.cache /contracts/node_modules /node_modules 

  # start server
  zkstack server
else
  zkstack chain create \
    --chain-name custom_token \
    --chain-id 275 \
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
    --chain custom_token \
    --validium-type no-da \
    --update-submodules=false
  
  rm -rf /usr/local/share/.cache /contracts/node_modules /node_modules
  # # If running in slave mode - wait for the master to be up and running.
  # echo "Waiting for zksync master to init hyperchain"
  # until curl --fail ${MASTER_HEALTH_URL}; do
  #   echo >&2 "Master zksync not ready yet, sleeping"
  #   sleep 5
  # done
  # start server
  zkstack server --chain custom_token 
fi
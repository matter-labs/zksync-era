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

if [ -n "$LEGACY_BRIDGE_TESTING" ]; then
  echo "LEGACY_BRIDGE_TESTING is set to $LEGACY_BRIDGE_TESTING"
fi

# Updates the value in the .toml config or .env file.
update_config() {
    # Assigning arguments to readable variable names
    local file="$1"
    local parameter="$2"
    local new_value="$3"
    local pattern_toml="^${parameter} =.*$"
    local pattern_env="^${parameter}=.*$"

    # Check if the parameter exists in the file
    if grep -q "$pattern_toml" "$file"; then
      # The parameter exists in the .toml file, so replace its value
      sed -i "s!$pattern_toml!${parameter} = \"$new_value\"!" "$file"
      echo "Update successful for $parameter in $file."
    elif grep -q "$pattern_env" "$file"; then
      # The parameter exists in the .env file, so replace its value
      sed -i "s!$pattern_env!${parameter}=$new_value!" "$file"
      echo "Update successful for $parameter in $file."
    else
      # The parameter does not exist in the file, output error message and return non-zero status
      echo "Error: '$parameter' not found in $file."
      return 1  # Return with an error status
    fi
}

# Reads the value of the parameter from the .toml config or .env file.
read_value_from_config() {
    local file="$1"
    local parameter="$2"
    local pattern_toml="^${parameter} =.*$"
    local pattern_env="^${parameter}=.*$"

    # Check if the parameter exists in the file
    if grep -q "$pattern_toml" "$file"; then
      # The parameter exists in the .toml file, so extract its value
      local value=$(grep "$pattern_toml" "$file" | sed "s/${parameter} = //; s/\"//g")
      echo "$value"
    elif grep -q "$pattern_env" "$file"; then
      # The parameter exists in the .env file, so extract its value
      local value=$(grep "$pattern_env" "$file" | sed "s/${parameter}=//; s/\"//g")
      echo "$value"
    else
      # The parameter does not exist in the file, output error message and return non-zero status
      echo "Error: '$parameter' not found in $file."
      return 1  # Return with an error status
    fi
}

# wait till db service is ready
until psql ${DATABASE_URL%/*} -c '\q'; do
  >&2 echo "Postgres is unavailable - sleeping"
  sleep 5
done


# Normally, the /etc/env and /var/lib/zksync/data should be mapped to volumes
# so that they are persisted between docker restarts - which would allow even faster starts.

# We use the existance of this init file to decide whether to restart or not.
INIT_FILE="/var/lib/zksync/data/INIT_COMPLETED.remove_to_reset"

if [ -f "$INIT_FILE" ]; then
    echo "Initialization was done in the past - simply starting server"
else
    echo "Initialing local environment"

    mkdir -p /var/lib/zksync/data

    update_config "/etc/env/base/private.toml" "database_url" "$DATABASE_URL"
    update_config "/etc/env/base/private.toml" "database_prover_url" "$DATABASE_PROVER_URL"
    update_config "/etc/env/base/eth_client.toml" "web3_url" "$ETH_CLIENT_WEB3_URL"
    # Put database in a special /var/lib directory so that it is persisted between docker runs.
    update_config "/etc/env/base/database.toml" "path" "/var/lib/zksync/data"
    update_config "/etc/env/base/database.toml" "state_keeper_db_path" "/var/lib/zksync/data/state_keeper"
    update_config "/etc/env/base/database.toml" "backup_path" "/var/lib/zksync/data/backups"

    if [ "$LEGACY_BRIDGE_TESTING" = "true" ]; then
      # making era chain id same as current chain id for legacy bridge testing
      FILE_PATH="/etc/env/base/chain.toml"
      PARAM_NAME="zksync_network_id"

      CHAIN_ETH_ZKSYNC_NETWORK_ID=$(read_value_from_config "$FILE_PATH" "$PARAM_NAME")

      if [ -z "$CHAIN_ETH_ZKSYNC_NETWORK_ID" ]; then
        echo "ERROR: $PARAM_NAME is not set in $FILE_PATH."
        exit 1
      fi

      update_config "/etc/env/base/contracts.toml" "ERA_CHAIN_ID" "$CHAIN_ETH_ZKSYNC_NETWORK_ID"
    fi

    zk config compile
  
    zk db reset
    
    # Perform initialization

    zk contract deploy-verifier
    zk run deploy-erc20 dev # (created etc/tokens/localhost)

    ## init bridgehub state transition
    zk contract deploy # (deploy L1)
    zk contract initialize-governance
    zk contract initialize-validator

    ## init hyperchain
    zk contract register-hyperchain
    zk f zksync_server --genesis


    DEPLOY_L2_PARAMS=""
    if [ "$LEGACY_BRIDGE_TESTING" = "true" ]; then
      # setting the flag for legacy bridge testing
      DEPLOY_L2_PARAMS="--local-legacy-bridge-testing"
    fi

    zk contract deploy-l2-through-l1 $DEPLOY_L2_PARAMS

    if [ "$LEGACY_BRIDGE_TESTING" = "true" ]; then
      # making era address same as current address for legacy bridge testing
      FILE_PATH="/etc/env/target/dev.env"
      PARAM_NAME="CONTRACTS_DIAMOND_PROXY_ADDR"

      CONTRACTS_DIAMOND_PROXY_ADDR=$(read_value_from_config "$FILE_PATH" "$PARAM_NAME")

      if [ -z "$CONTRACTS_DIAMOND_PROXY_ADDR" ]; then
        echo "ERROR: $PARAM_NAME is not set in $FILE_PATH."
        exit 1
      fi

      update_config "/etc/env/target/dev.env" "CONTRACTS_ERA_DIAMOND_PROXY_ADDR" "$CONTRACTS_DIAMOND_PROXY_ADDR"
    fi
  
    zk f yarn --cwd /infrastructure/local-setup-preparation start

    # Create init file.
    echo "System initialized. Please remove this file if you want to reset the system" > $INIT_FILE

fi

if [ "$LEGACY_BRIDGE_TESTING" = "true" ]; then
  # setup-legacy-bridge-era waits for the server to be ready, so starting it in the background
  zk contract setup-legacy-bridge-era &
fi

# start server
zk f zksync_server

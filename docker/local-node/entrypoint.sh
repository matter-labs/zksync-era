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
    return 1 # Return with an error status
  fi
}

# Reads the value of the parameter from the .toml config or .env file.
read_value_from_config() {
  local file="$1"
  local parameter="$2"
  local pattern_toml="^${parameter} =.*$"
  local pattern_env="^${parameter}=.*$"
  local res=""

  # Check if the parameter exists in the file
  if grep -q "$pattern_toml" "$file"; then
    # The parameter exists in the .toml file, so extract its value
    res=$(grep "$pattern_toml" "$file" | sed "s/${parameter} = //; s/\"//g")
  elif grep -q "$pattern_env" "$file"; then
    # The parameter exists in the .env file, so extract its value
    res=$(grep "$pattern_env" "$file" | sed "s/${parameter}=//; s/\"//g")
  else
    # The parameter does not exist in the file, output error message and return non-zero status
    echo "Error: '$parameter' not found in $file."
    return 1 # Return with an error status
  fi

  if [ -z "$res" ]; then
    echo "ERROR: $parameter is not set in $file."
    exit 1
  fi

  echo $res
}

# wait till db service is ready
until psql ${DATABASE_URL%/*} -c '\q'; do
  echo >&2 "Postgres is unavailable - sleeping"
  sleep 5
done

if [ -z "$MASTER_URL" ]; then
  echo "Running as zksync master"
else
  # If running in slave mode - wait for the master to be up and running.
  echo "Waiting for zksync master to init hyperchain"
  until curl --fail ${MASTER_HEALTH_URL}; do
    echo >&2 "Master zksync not ready yet, sleeping"
    sleep 5
  done
fi

if [ -n "$LEGACY_BRIDGE_TESTING" ]; then
  if [ -z "$MASTER_URL" ]; then
    echo "Running in legacy bridge testing mode"
  else
    # LEGACY_BRIDGE_TESTING flag is for the master only
    unset LEGACY_BRIDGE_TESTING
  fi
fi

# Normally, the /etc/env and /var/lib/zksync/data should be mapped to volumes
# so that they are persisted between docker restarts - which would allow even faster starts.

# We use the existance of this init file to decide whether to restart or not.
INIT_FILE="/var/lib/zksync/data/INIT_COMPLETED.remove_to_reset"

if [ -f "$INIT_FILE" ]; then
  echo "Initialization was done in the past - simply starting server"
else
  echo "Initializing local environment"

  mkdir -p /var/lib/zksync/data

  update_config "/etc/env/base/private.toml" "database_url" "$DATABASE_URL"
  update_config "/etc/env/base/private.toml" "database_prover_url" "$DATABASE_PROVER_URL"
  update_config "/etc/env/base/eth_client.toml" "web3_url" "$ETH_CLIENT_WEB3_URL"
  # Put database in a special /var/lib directory so that it is persisted between docker runs.
  update_config "/etc/env/base/database.toml" "path" "/var/lib/zksync/data"
  update_config "/etc/env/base/database.toml" "state_keeper_db_path" "/var/lib/zksync/data/state_keeper"
  update_config "/etc/env/base/database.toml" "backup_path" "/var/lib/zksync/data/backups"

  if [ -n "$LEGACY_BRIDGE_TESTING" ]; then
    # making era chain id same as current chain id for legacy bridge testing
    chain_eth_zksync_network_id=$(read_value_from_config "/etc/env/base/chain.toml" "zksync_network_id")
    update_config "/etc/env/base/contracts.toml" "ERA_CHAIN_ID" "$chain_eth_zksync_network_id"
  fi

  if [ -z "$MASTER_URL" ]; then
    echo "Starting with hyperchain"
  else
    # Updates all the stuff (from the '/etc/master_env') - it assumes that it is mapped via docker compose.
    zk f yarn --cwd /infrastructure/local-setup-preparation join
  fi

  zk config compile

  zk db reset

  # Perform initialization (things needed to be done only if you're running in the master mode)
  if [ -z "$MASTER_URL" ]; then
    zk contract deploy-verifier
    zk run deploy-erc20 dev # (created etc/tokens/localhost)

    ## init bridgehub state transition
    zk contract deploy # (deploy L1)
    zk contract initialize-governance
    zk contract initialize-validator
  fi


  if [ -z "$CUSTOM_BASE_TOKEN" ]; then
    echo "Starting chain with ETH as gas token"

    if [ -z "$VALIDIUM_MODE" ]; then
      ## init hyperchain in rollup mode
      zk contract register-hyperchain
    else
      zk contract register-hyperchain --deployment-mode 1
    fi
  else
    echo "Starting chain with custom gas token $CUSTOM_BASE_TOKEN"
    zk contract register-hyperchain --base-token-name $CUSTOM_BASE_TOKEN
  fi
  
  
  zk f zksync_server --genesis

  deploy_l2_args=""
  if [ -n "$LEGACY_BRIDGE_TESTING" ]; then
    # setting the flag for legacy bridge testing
    deploy_l2_args="--local-legacy-bridge-testing"
  fi

  zk contract deploy-l2-through-l1 $deploy_l2_args

  if [ -z "$MASTER_URL" ]; then
    zk f yarn --cwd /infrastructure/local-setup-preparation start
  fi

  if [ -n "$LEGACY_BRIDGE_TESTING" ]; then
    # making era address same as current address for legacy bridge testing
    contracts_diamond_proxy_addr=$(read_value_from_config "/etc/env/target/dev.env" "CONTRACTS_DIAMOND_PROXY_ADDR")
    update_config "/etc/env/target/dev.env" "CONTRACTS_ERA_DIAMOND_PROXY_ADDR" "$contracts_diamond_proxy_addr"

    # setup-legacy-bridge-era waits for the server to be ready, so starting it in the background
    zk contract setup-legacy-bridge-era &
  fi

  # Create init file.
  echo "System initialized. Please remove this file if you want to reset the system" >$INIT_FILE

fi

# start server
zk f zksync_server

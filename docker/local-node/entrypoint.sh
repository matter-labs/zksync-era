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

# Updates the value in the .toml config.
update_config() {
    # Assigning arguments to readable variable names
    local file="$1"
    local parameter="$2"
    local new_value="$3"
    local pattern="^${parameter} =.*$"

    # Check if the parameter exists in the file
    if grep -q "$pattern" "$file"; then
        # The parameter exists, so replace its value
        sed -i "s!$pattern!${parameter} =\"${new_value}\"!" "$file"
        echo "Update successful for $parameter in $file."
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

    zk config compile
  

    zk db drop
    zk db setup


    # Perform initialization

    zk contract deploy --only-verifier
    zk f zksync_server --genesis
    zk run deploy-erc20 dev # (created etc/tokens/localhost)
    zk contract deploy # (deploys rest of stuff)

    zk contract initialize-validator 

    zk contract deploy-l2
    zk contract initialize-governance

    zk f yarn --cwd /infrastructure/local-setup-preparation start

    # Create init file.
    echo "System initialized. Please remove this file if you want to reset the system" > $INIT_FILE

fi

# start server
zk f zksync_server

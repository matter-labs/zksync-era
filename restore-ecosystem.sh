#!/bin/bash

# Check if the ecosystem name was provided as an argument
if [ -z "$1" ]; then
  echo "Usage: ./restore-ecosystem ECOSYSTEM_NAME"
  exit 1
fi

# Store the first argument as ECOSYSTEM_NAME
ECOSYSTEM_NAME=$1

# Prompt for the Postgres password and store it in PGPASSWORD
read -sp "Enter Postgres password: " PGPASSWORD
export PGPASSWORD

# Check if the chain directory exists
CHAIN_PATH="./chains/${ECOSYSTEM_NAME}"
BACKUP_PATH="./ecosystem_backups/${ECOSYSTEM_NAME}"

# Check if the backup directory exists
if [ ! -d "$BACKUP_PATH" ]; then
  echo "Error: Backup not found at $BACKUP_PATH."
  exit 1
fi

# Check if the postgres is running
if ! docker ps --filter "name=postgres" --filter "status=running" | grep "postgres" > /dev/null; then
  echo "Error: postgres not running, set it up first with 'zk_inception containers'."
  exit 1
fi

# Fix backup files $ZKSYNC_HOME paths
find_and_replace() {
  local target_file=$1

  sed -i '' -e "s|db_path:.*zksync-era/\./|db_path: $(pwd)/./|g" "$target_file"
  sed -i '' -e "s|state_keeper_db_path:.*zksync-era/\./|state_keeper_db_path: $(pwd)/./|g" "$target_file"
  sed -i '' -e "s|path:.*zksync-era/\./|path: $(pwd)/./|g" "$target_file"
  sed -i '' -e "s|configs:.*zksync-era/\./|configs: $(pwd)/./|g" "$target_file"
}

# Array of specific files to modify
files=("$BACKUP_PATH/$ECOSYSTEM_NAME/configs/general.yaml" "$BACKUP_PATH/$ECOSYSTEM_NAME/ZkStack.yaml")

# Loop over the files and perform the find and replace
for file in "${files[@]}"; do
  if [ -f "$file" ]; then
    find_and_replace "$file"
  else
    # Exit with error code
    echo "ERROR: backup file $file does not exist."
    exit 1
  fi
done

# Copy the ecosystem backup folder to the chains folder, replacing any existing files
echo "Copying backup files to $CHAIN_PATH..."
cp -r "$BACKUP_PATH/$ECOSYSTEM_NAME" "$CHAIN_PATH"

# Copy the configs folder in the backup to the configs folder in the root of the project
# TODO: it may be suitable to warn the user about overwriting the existing configs
# and ask for confirmation before proceeding
echo "Copying configs folder from backup..."
cp -r "$BACKUP_PATH/configs" "./"

# Path to the secrets.yaml file
SECRETS_FILE="$CHAIN_PATH/configs/secrets.yaml"

# Check if the secrets.yaml file exists
if [ ! -f "$SECRETS_FILE" ]; then
  echo "Error: $SECRETS_FILE does not exist."
  exit 1
fi

# Extract server_url and prover_url from the secrets.yaml file
SERVER_DB_NAME=$(grep 'server_url' "$SECRETS_FILE" | awk -F'/' '{print $NF}')
PROVER_DB_NAME=$(grep 'prover_url' "$SECRETS_FILE" | awk -F'/' '{print $NF}')

# Show the extracted database names
echo "Extracted SERVER_DB_NAME: $SERVER_DB_NAME"
echo "Extracted PROVER_DB_NAME: $PROVER_DB_NAME"

# Create and restore the server database
echo "Creating database $SERVER_DB_NAME..."
createdb -U postgres -h localhost "$SERVER_DB_NAME"

echo "Restoring $SERVER_DB_NAME from backup..."
psql -U postgres -h localhost -d "$SERVER_DB_NAME" -f "$BACKUP_PATH/${SERVER_DB_NAME}_backup.sql"

# Create and restore the prover database
echo "Creating database $PROVER_DB_NAME..."
createdb -U postgres -h localhost "$PROVER_DB_NAME"

echo "Restoring $PROVER_DB_NAME from backup..."
psql -U postgres -h localhost -d "$PROVER_DB_NAME" -f "$BACKUP_PATH/${PROVER_DB_NAME}_backup.sql"

# Unset the PGPASSWORD variable for security
unset PGPASSWORD

echo "Restore completed."

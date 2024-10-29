#!/bin/bash

# This script backs up the Postgres databases and chain configuration files for a given ecosystem. 
# With it you can make a testnet deployment and save the L2 state for later recovery.

# Check if the ecosystem name was provided as an argument
if [ -z "$1" ]; then
  echo "Usage: ./backup-ecosystem ECOSYSTEM_NAME"
  exit 1
fi

# Store the first argument as ECOSYSTEM_NAME
ECOSYSTEM_NAME=$1

# Prompt for the Postgres password and store it in PGPASSWORD
read -sp "Enter Postgres password: " PGPASSWORD
export PGPASSWORD

# Path to the secrets.yaml file
SECRETS_FILE="./chains/${ECOSYSTEM_NAME}/configs/secrets.yaml"

# Check if the secrets.yaml file exists
if [ ! -f "$SECRETS_FILE" ]; then
  echo "Error: $SECRETS_FILE does not exist."
  exit 1
fi

# Extract server_url and prover_url from the secrets.yaml file
SERVER_DB_NAME=$(grep 'server_url' "$SECRETS_FILE" | awk -F'/' '{print $NF}')
PROVER_DB_NAME=$(grep 'prover_url' "$SECRETS_FILE" | awk -F'/' '{print $NF}')

# Export the database names
echo "Extracted SERVER_DB_NAME: $SERVER_DB_NAME"
echo "Extracted PROVER_DB_NAME: $PROVER_DB_NAME"

# Create backup directory
mkdir -p "./ecosystem_backups/${ECOSYSTEM_NAME}"

# Run pg_dump commands
echo "Running pg_dump for $SERVER_DB_NAME..."
pg_dump -U postgres -h localhost "$SERVER_DB_NAME" > "ecosystem_backups/${ECOSYSTEM_NAME}/${SERVER_DB_NAME}_backup.sql"
echo "Running pg_dump for $PROVER_DB_NAME..."
pg_dump -U postgres -h localhost "$PROVER_DB_NAME" > "ecosystem_backups/${ECOSYSTEM_NAME}/${PROVER_DB_NAME}_backup.sql"

# Unset the PGPASSWORD variable for security
unset PGPASSWORD

# Copy the chain configuration files
cp -r "./chains/${ECOSYSTEM_NAME}" "./ecosystem_backups/${ECOSYSTEM_NAME}/"

# Copy the configs directory
cp -r "./configs" "./ecosystem_backups/${ECOSYSTEM_NAME}/"

echo "Backup completed."

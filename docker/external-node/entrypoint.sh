#!/bin/bash

set -e

# Prepare the database if it's not ready. No-op if the DB is prepared.
sqlx database setup
# Run the external node.
exec zksync_external_node "$@"

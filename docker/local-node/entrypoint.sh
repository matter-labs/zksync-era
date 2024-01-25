#!/bin/bash
set -ea

# wait till db service is ready
until psql ${DATABASE_URL%/*} -c '\q'; do
  >&2 echo "Postgres is unavailable - sleeping"
  sleep 5
done

# ensure database initialization
if ! psql $DATABASE_URL -c '\q' 2>/dev/null;
then
    echo "Initialing local environment"
    psql ${DATABASE_URL%/*} -c "create database ${DATABASE_URL##*/}"
    find /migrations -name "*up.sql" | sort | xargs printf -- ' -f %s' | xargs -t psql $DATABASE_URL

    cd /infrastructure/zk
    # Compile configs
    yarn start config compile

    # Override values for database URL and eth client in the toml config files
    # so they will be taken into account
    sed -i 's!^database_url=.*$!database_url="'"$DATABASE_URL"'"!' /etc/env/base/private.toml
    sed -i 's!^web3_url=.*$!web3_url="'"$ETH_CLIENT_WEB3_URL"'"!' /etc/env/base/eth_client.toml
    sed -i 's!^path=.*$!path="/var/lib/zksync/data"!' /etc/env/base/database.toml
    sed -i 's!^state_keeper_db_path=.*$!state_keeper_db_path="/var/lib/zksync/data/state_keeper"!' /etc/env/base/database.toml
    sed -i 's!^merkle_tree_backup_path=.*$!merkle_tree_backup_path="/var/lib/zksync/data/backups"!' /etc/env/base/database.toml

    # Switch zksolc compiler source from docker to binary
    sed -i "s!'docker'!'binary'!" /contracts/l2-contracts/hardhat.config.ts

    # Compile configs again (with changed values)
    yarn start config compile

    # Perform initialization
    yarn start lightweight-init
    yarn start f yarn --cwd /infrastructure/local-setup-preparation start

    # Return to the root directory
    cd /
fi

# start server
source /etc/env/dev.env
source /etc/env/.init.env
exec zksync_server

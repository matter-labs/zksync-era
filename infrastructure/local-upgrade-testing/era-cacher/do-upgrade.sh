#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
WORKING_DIR="$WORKSPACE_ROOT"
HELPER_DIR="$WORKSPACE_ROOT/infrastructure/local-upgrade-testing/era-cacher"

wait_for_rpc() {
    local url="$1"
    local label="$2"
    local max_attempts="${3:-180}"
    local sleep_seconds="${4:-2}"

    for ((attempt = 1; attempt <= max_attempts; attempt++)); do
        if curl -sSf -X POST "$url" \
            -H 'Content-Type: application/json' \
            --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
            >/dev/null; then
            echo "$label RPC is ready at $url"
            return 0
        fi
        sleep "$sleep_seconds"
    done

    echo "Timed out waiting for $label RPC at $url" >&2
    return 1
}

"$HELPER_DIR/reset.sh"

"$HELPER_DIR/use-old-era.sh"
cd "$WORKING_DIR"

upgrade_version="v31-interop-b"

# We delete information about the gateway chain as presence of old chain configs
# changes the beavior of zkstack ecosystem init.
[ -d "./chains/gateway" ] && rm -rf "./chains/gateway"

zkstackup --local && zkstack dev clean containers && zkstack up --observability false

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --verbose \
    --chain era \
    --observability=false

zkstack server --chain era &> "$WORKSPACE_ROOT/../rollup3.log" &

wait_for_rpc http://127.0.0.1:3050 "pre-upgrade era"

zkstack dev rich-account 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 --chain era

# When running locally, open a new terminal window here.
cd "$WORKSPACE_ROOT/.."
"$HELPER_DIR/use-new-era.sh"
cd "$WORKING_DIR"

zkstackup --local
zkstack dev contracts

zkstack dev database migrate --prover false --core true --chain era

# Keep the old-tree server active until the onchain upgrade reaches the new ABI.
# The new-tree runtime cannot start safely against pre-upgrade v29 chain state.

# Update permanent-values.toml with addresses from running deployment
"$HELPER_DIR/update-permanent-values.sh"

zkstack dev run-ecosystem-upgrade --upgrade-version "$upgrade_version" --ecosystem-upgrade-stage no-governance-prepare
zkstack dev run-ecosystem-upgrade --upgrade-version "$upgrade_version" --ecosystem-upgrade-stage ecosystem-admin
zkstack dev run-ecosystem-upgrade --upgrade-version "$upgrade_version" --ecosystem-upgrade-stage governance-stage0
zkstack dev run-ecosystem-upgrade --upgrade-version "$upgrade_version" --ecosystem-upgrade-stage governance-stage1

cd "$WORKING_DIR/contracts/l1-contracts"
UPGRADE_ECOSYSTEM_OUTPUT=script-out/v31-upgrade-ecosystem.toml \
UPGRADE_ECOSYSTEM_OUTPUT_TRANSACTIONS=broadcast/EcosystemUpgrade_v31.s.sol/9/1658e297-latest.json \
YAML_OUTPUT_FILE=script-out/v31-local-output.yaml \
    yarn upgrade-yaml-output-generator
cd "$WORKING_DIR"

# Governance stage 1 updates canonical contracts config (including BytecodesSupplier).
# Re-sync permanent values before regenerating the per-chain runtime config.
"$HELPER_DIR/update-permanent-values.sh"

# Refresh generated chain configs from the now-updated canonical contracts config.
# The node reads chains/era/configs/contracts.yaml on restart.
zkstack dev contracts

zkstack dev run-chain-upgrade --upgrade-version "$upgrade_version" --force-display-finalization-params=true --dangerous-local-default-overrides=true --chain era
zkstack dev run-ecosystem-upgrade --upgrade-version "$upgrade_version" --ecosystem-upgrade-stage governance-stage2

# v31 SettlementLayerV31UpgradeBase intentionally resets s.l1DAValidator and s.l2DACommitmentScheme.
# The operator must re-set the pair for eth_tx_aggregator to commit batches again.
L1_DA_VALIDATOR=$(awk '/^  rollup_l1_da_validator_addr:/ { print $2; exit }' \
    "$WORKING_DIR/chains/era/configs/contracts.yaml")
if [ -z "$L1_DA_VALIDATOR" ]; then
    echo "Could not read rollup_l1_da_validator_addr from runtime config" >&2
    exit 1
fi
zkstack chain set-da-validator-pair "$L1_DA_VALIDATOR" BlobsAndPubdataKeccak256 --chain era

# Stage 3: Migrate token balances from NTV to AssetTracker.
# Read the live Bridgehub proxy from the refreshed runtime config instead of hardcoding it.
BRIDGEHUB_PROXY=$(awk '/^  bridgehub_proxy_addr:/ { print $2; exit }' \
    "$WORKING_DIR/chains/era/configs/contracts.yaml")
if [ -z "$BRIDGEHUB_PROXY" ]; then
    echo "Could not read bridgehub_proxy_addr from runtime config" >&2
    exit 1
fi
cd "$WORKING_DIR/contracts/l1-contracts"
forge script deploy-scripts/upgrade/v31/EcosystemUpgrade_v31.s.sol:EcosystemUpgrade_v31 \
    --sig "stage3(address)" \
    "$BRIDGEHUB_PROXY" \
    --rpc-url http://localhost:8545 \
    --broadcast \
    --private-key 0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 \
    --legacy \
    --slow \
    --gas-price 50000000000
cd "$WORKING_DIR"

pkill -9 zksync_server || true
zkstack server --ignore-prerequisites --chain era &> "$WORKSPACE_ROOT/../rollup2.log" &

wait_for_rpc http://127.0.0.1:3050 "post-upgrade era"

# Fund the main wallet (test_mnemonic index 0) with L1 ETH.
# This wallet is used by init-test-wallet to distribute to the actual test wallet (index 101).
cast send 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 \
    --value 10001ether \
    --rpc-url http://127.0.0.1:8545 \
    --private-key 0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 \
    --gas-price 50gwei

zkstack dev init-test-wallet --chain era
zkstack dev test integration --no-deps --ignore-prerequisites --chain era

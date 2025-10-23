#!/usr/bin/env bash
set -Eeuo pipefail

########################################
# Config (tweak as needed)
########################################
L1_RPC="http://127.0.0.1:8545"

# DB (use distinct names per chain)
DB_URL="postgres://postgres:notsecurepassword@localhost:5432"
DB_ERA_NAME="zksync_server_localhost_era"
DB_VALIDIUM_NAME="zksync_server_localhost_validium"
DB_GATEWAY_NAME="zksync_server_localhost_gateway"

# Chains
ERA_CHAIN="era"
VALIDIUM_CHAIN="validium"
GATEWAY_CHAIN="gateway"
VALIDIUM_ID=260
GATEWAY_ID=506

# Ports for health checks (adjust to your local config)
ERA_HEALTH="http://127.0.0.1:3071/health"
VALIDIUM_HEALTH="http://127.0.0.1:3271/health"
GATEWAY_HEALTH="http://127.0.0.1:3171/health"

# Misc
LOG_DIR="./zruns"
ZLOG_DIR="./zlogs"
GATEWAY_CFG_PATH="./chains/${GATEWAY_CHAIN}/configs/gateway.yaml"   # where zkstack expects it
RETRY_MAX=120         # 120 * 0.25s = 30s wait max per health check
SLEEP_STEP="0.25"

########################################
# Helpers
########################################
die() { echo "❌  $*" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || die "Missing required command: $1"
}

log() {
  echo "• $*"
}

mklogs() {
  mkdir -p "$LOG_DIR" "$ZLOG_DIR" "./chains/${GATEWAY_CHAIN}/configs" "./etc/env/file_based/overrides/tests"
}

curl_ok() {
  curl -fsS "$1" >/dev/null 2>&1
}

wait_health() {
  local name="$1" url="$2"
  log "Waiting for $name to be healthy at $url ..."
  local i=0
  until curl_ok "$url"; do
    ((i++))
    if (( i > RETRY_MAX )); then
      die "$name did not become healthy at $url in time"
    fi
    sleep "$SLEEP_STEP"
  done
  log "$name is healthy ✓"
}

kill_servers() {
  log "Stopping any running zksync_server processes…"
  pkill -9 -f zksync_server >/dev/null 2>&1 || true
}

trap 'echo; echo "⚠️  Trapped error; collecting recent logs…"; ls -lah "$LOG_DIR" || true; exit 1' ERR

########################################
# Pre-flight
########################################
need zkstack
need curl
need jq || true      # not required, but handy if you log JSON
mklogs
kill_servers

# Quick sanity on bridge helper scripts
[[ -x ./infrastructure/scripts/bridge_eth_to_era.sh ]]      || die "bridge_eth_to_era.sh not found/executable"
[[ -x ./infrastructure/scripts/bridge_token_to_era.sh ]]    || die "bridge_token_to_era.sh not found/executable"
[[ -x ./infrastructure/scripts/bridge_token_from_era.sh ]]  || die "bridge_token_from_era.sh not found/executable"

########################################
# Stage 1: Clean & bring up base stack
########################################
log "Resetting local stack…"
zkstack dev clean containers || true
zkstack up -o false
zkstack dev contracts

# This generates genesis for ERA (root chain)
zkstack dev generate-genesis

# Ecosystem init (ERA)
log "Initializing ecosystem for ERA…"
zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
  --deploy-ecosystem --l1-rpc-url="${L1_RPC}" \
  --server-db-url="${DB_URL}" \
  --server-db-name="${DB_ERA_NAME}" \
  --ignore-prerequisites --observability=false \
  --chain "${ERA_CHAIN}" 

########################################
# Stage 2: Create + init Validium & Gateway (config before servers!)
########################################
log "Creating VALIDIUM chain…"
zkstack chain create \
  --chain-name "${VALIDIUM_CHAIN}" \
  --chain-id "${VALIDIUM_ID}" \
  --prover-mode no-proofs \
  --wallet-creation localhost \
  --l1-batch-commit-data-generator-mode validium \
  --base-token-address 0x0000000000000000000000000000000000000001 \
  --base-token-price-nominator 1 \
  --base-token-price-denominator 1 \
  --set-as-default false \
  --evm-emulator false \
  --ignore-prerequisites

log "Initializing VALIDIUM chain…"
zkstack chain init \
  --deploy-paymaster \
  --l1-rpc-url="${L1_RPC}" \
  --server-db-url="${DB_URL}" \
  --server-db-name="${DB_VALIDIUM_NAME}" \
  --chain "${VALIDIUM_CHAIN}" \
  --validium-type no-da 

log "Creating GATEWAY chain…"
zkstack chain create \
  --chain-name "${GATEWAY_CHAIN}" \
  --chain-id "${GATEWAY_ID}" \
  --prover-mode no-proofs \
  --wallet-creation localhost \
  --l1-batch-commit-data-generator-mode rollup \
  --base-token-address 0x0000000000000000000000000000000000000001 \
  --base-token-price-nominator 1 \
  --base-token-price-denominator 1 \
  --set-as-default false \
  --evm-emulator false \
  --ignore-prerequisites

log "Initializing GATEWAY chain…"
zkstack chain init \
  --deploy-paymaster \
  --l1-rpc-url="${L1_RPC}" \
  --server-db-url="${DB_URL}" \
  --server-db-name="${DB_GATEWAY_NAME}" \
  --chain "${GATEWAY_CHAIN}" 

########################################
# Stage 3: Start ERA, fund + bridge tokens
########################################
log "Starting ERA server…"
zkstack server --ignore-prerequisites --chain "${ERA_CHAIN}" &> "${LOG_DIR}/era1.log" &
sleep 0.5
zkstack server wait --ignore-prerequisites --verbose --chain "${ERA_CHAIN}"
wait_health "ERA" "${ERA_HEALTH}"

# Bridge flows (these scripts depend on ERA being live)
log "Bridging ETH → ERA…"
sh ./infrastructure/scripts/bridge_eth_to_era.sh "${ERA_CHAIN}"
log "Bridging test token → ERA…"
sh ./infrastructure/scripts/bridge_token_to_era.sh "${ERA_CHAIN}"

# Optional: test a token bridge from ERA (wait a touch for receipts)
sleep 2
log "Bridging token FROM ERA (smoke)…"
sh ./infrastructure/scripts/bridge_token_from_era.sh "${ERA_CHAIN}"
sleep 2

# Clean restart before Gateway ops (avoids sticky ports/file locks)
kill_servers
sleep 2

########################################
# Stage 4: Prepare Gateway mode BEFORE starting server
########################################
log "Preparing Gateway mode transforms…"
# Create filterer + convert to gateway
zkstack chain gateway create-tx-filterer --chain "${GATEWAY_CHAIN}" --ignore-prerequisites
zkstack chain gateway convert-to-gateway --chain "${GATEWAY_CHAIN}" --ignore-prerequisites

# Write gateway config (this produces the gateway.yaml needed by the server)
log "Writing gateway config file…"
zkstack dev config-writer \
  --path "etc/env/file_based/overrides/tests/gateway.yaml" \
  --chain "${GATEWAY_CHAIN}"

# Ensure the expected path exists & copy into the chain config location if needed
if [[ ! -f "${GATEWAY_CFG_PATH}" ]]; then
  log "Copying gateway.yaml into ${GATEWAY_CFG_PATH} (zkstack expects it here)…"
  mkdir -p "$(dirname "${GATEWAY_CFG_PATH}")"
  cp -f "etc/env/file_based/overrides/tests/gateway.yaml" "${GATEWAY_CFG_PATH}"
fi

# Sanity check gateway.yaml presence
[[ -f "${GATEWAY_CFG_PATH}" ]] || die "Gateway config missing at ${GATEWAY_CFG_PATH}"

########################################
# Stage 5: Start Gateway server (now that config exists)
########################################
log "Starting GATEWAY server…"
zkstack server --ignore-prerequisites --chain "${GATEWAY_CHAIN}" &> "${LOG_DIR}/gateway.log" &
sleep 0.5
zkstack server wait --ignore-prerequisites --verbose --chain "${GATEWAY_CHAIN}"
wait_health "GATEWAY" "${GATEWAY_HEALTH}"

########################################
# Stage 6: Migrate to gateway + bring chains up again
########################################
log "Migrating ERA and VALIDIUM to use GATEWAY…"
zkstack chain gateway migrate-to-gateway --chain "${ERA_CHAIN}" --gateway-chain-name "${GATEWAY_CHAIN}"
zkstack chain gateway migrate-to-gateway --chain "${VALIDIUM_CHAIN}" --gateway-chain-name "${GATEWAY_CHAIN}"

log "Starting ERA + VALIDIUM servers after migration…"
zkstack server --ignore-prerequisites --chain "${ERA_CHAIN}" &> "${LOG_DIR}/era.log" &
zkstack server --ignore-prerequisites --chain "${VALIDIUM_CHAIN}" &> "${LOG_DIR}/validium.log" &
sleep 0.5
zkstack server wait --ignore-prerequisites --verbose --chain "${ERA_CHAIN}"
zkstack server wait --ignore-prerequisites --verbose --chain "${VALIDIUM_CHAIN}"
wait_health "ERA" "${ERA_HEALTH}"
wait_health "VALIDIUM" "${VALIDIUM_HEALTH}"

########################################
# Stage 7: Token balance migration → Gateway
########################################
log "Migrating token balances to GATEWAY…"
zkstack chain gateway migrate-token-balances --to-gateway --chain "${ERA_CHAIN}" --gateway-chain-name "${GATEWAY_CHAIN}"
zkstack chain gateway migrate-token-balances --to-gateway --chain "${VALIDIUM_CHAIN}" --gateway-chain-name "${GATEWAY_CHAIN}"

########################################
# Stage 8: Init test wallets (optional)
########################################
log "Initializing test wallets…"
zkstack dev init-test-wallet --chain "${ERA_CHAIN}" || true
zkstack dev init-test-wallet --chain "${VALIDIUM_CHAIN}" || true

log "✅ Spin-up complete. Logs: ${LOG_DIR}"

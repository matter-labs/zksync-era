#!/bin/bash
#
# Era v29 → v31 local upgrade flow, driven by `protocol-ops` for the upgrade
# orchestration steps that protocol-ops covers, and zkstack for surrounding
# lifecycle (ecosystem init, server, db migrate, integration tests).
#
# protocol-ops covers:
#   - `ecosystem upgrade-prepare` (deploys new contracts; emits Safe bundle)
#   - `ecosystem upgrade-governance` (runs gov stages 0+1+2 bundled)
#   - `chain set-upgrade-timestamp` (schedules the L2 upgrade tx)
#   - `chain upgrade` (chain-level Safe bundle: executeUpgrade)
#   - `chain set-da-validator-pair` (re-set after v31 wipes the pair)
#
# Each phase passes `--execute --wallets-yaml ECO --wallets-yaml CHAIN` so
# protocol-ops both prepares and dispatches the Safe bundle in-process,
# looking up signers by target address across both zkstack wallets.yaml
# files.
#
# zkstack still drives:
#   - `ecosystem init` (v29 deploy + DB + wallets — protocol-ops doesn't
#     exist at v29, so we must use zkstack to seed the v29 state)
#   - `server`, `dev contracts`, `dev database migrate`
#   - `dev test integration` (suite runner)
#   - `dev rich-account`, `dev init-test-wallet` (test fixture setup)
#
# Direct cast / forge:
#   - `EcosystemUpgrade_v31:stage3` (no `protocol-ops` command for stage3).
#   - `cast call DiamondProxy.getProtocolVersion()` (post-upgrade
#     verification, mirrors OS ref's `verify_protocol_version`).
#
# Mirrors the ZKsync OS reference at
# `zksync-os-integration-tests-main/integration-tests/tests/upgrade_v30_to_v31.rs`,
# adapted for Era's v29 starting state.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_PARENT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKING_DIR="$WORKSPACE_PARENT/zksync-working"
NEW_REPO="$WORKSPACE_PARENT/zksync-new"

L1_RPC_URL="http://127.0.0.1:8545"
L2_RPC_URL="http://127.0.0.1:3050"

PROTOCOL_OPS_WORK="$WORKSPACE_PARENT/protocol-ops-work"
OLD_ZKSYNC_REF="${ERA_CACHER_OLD_ZKSYNC_REF:-a273232b3b11c1382209bf52754d91f9dfdb2f17}"

forge_version_line() {
    local forge_bin="$1"
    "$forge_bin" --version | head -n 1
}

is_old_v29_forge() {
    local forge_bin="$1"
    local version
    version="$(forge_version_line "$forge_bin" 2>/dev/null || true)"
    [[ "$version" =~ ^forge\ 0\.0\.4\ \(((ae913af65)|VERGEN_IDEMPOTENT_OUTPUT).*\)$ ]]
}

is_new_v31_forge() {
    local forge_bin="$1"
    local version
    version="$(forge_version_line "$forge_bin" 2>/dev/null || true)"
    [[ "$version" == "forge Version: 1.3.5-foundry-zksync-v0.1.5" ]]
}

resolve_foundry_bin_dir() {
    local kind="$1"
    local env_var="$2"
    local matcher="$3"
    shift 3

    local candidate="${!env_var:-}"
    if [ -n "$candidate" ]; then
        if [ -x "$candidate/forge" ] && "$matcher" "$candidate/forge"; then
            echo "$candidate"
            return 0
        fi
        echo "$env_var points at $candidate, but $candidate/forge is not the expected $kind forge" >&2
        exit 1
    fi

    for candidate in "$@"; do
        if [ -x "$candidate/forge" ] && "$matcher" "$candidate/forge"; then
            echo "$candidate"
            return 0
        fi
    done

    local forge_bin
    forge_bin="$(command -v forge || true)"
    if [ -n "$forge_bin" ] && "$matcher" "$forge_bin"; then
        dirname "$forge_bin"
        return 0
    fi

    echo "Could not find the expected $kind foundry-zksync forge binary." >&2
    echo "Set $env_var to a directory containing forge/cast for that version." >&2
    exit 1
}

setup_foundry_dispatch() {
    local old_foundry_bin_dir
    local new_foundry_bin_dir
    old_foundry_bin_dir="$(resolve_foundry_bin_dir \
        "v29" \
        "ERA_CACHER_OLD_FOUNDRY_BIN_DIR" \
        is_old_v29_forge \
        "$HOME/.foundry/versions/foundry-zksync-ae913af65" \
        "$HOME/.foundry/matter-labs/foundry-zksync/target/release")"
    new_foundry_bin_dir="$(resolve_foundry_bin_dir \
        "v31" \
        "ERA_CACHER_NEW_FOUNDRY_BIN_DIR" \
        is_new_v31_forge \
        "$HOME/.foundry/versions/foundry-zksync-v0.1.5" \
        "$HOME/.foundry/bin")"

    for tool in forge cast; do
        [ -x "$old_foundry_bin_dir/$tool" ] || { echo "Missing $old_foundry_bin_dir/$tool" >&2; exit 1; }
        [ -x "$new_foundry_bin_dir/$tool" ] || { echo "Missing $new_foundry_bin_dir/$tool" >&2; exit 1; }
    done

    local dispatch_dir="$WORKSPACE_PARENT/foundry-dispatch"
    mkdir -p "$dispatch_dir"
    for tool in forge cast; do
        cat > "$dispatch_dir/$tool" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [ -d "$ERA_CACHER_WORKING_DIR/.git" ] \
    && [ "$(git -C "$ERA_CACHER_WORKING_DIR" rev-parse HEAD)" = "$ERA_CACHER_OLD_ZKSYNC_REF" ]; then
    if [ "TOOL" = "forge" ] && [ "${1:-}" = "script" ]; then
        case "${2:-}" in
            deploy-scripts/AdminFunctions.s.sol)
                set -- "$1" "$2:AdminFunctions" "${@:3}"
                ;;
            deploy-scripts/DeployCTM.s.sol | deploy-scripts/ctm/DeployCTM.s.sol)
                set -- "$1" "$2:DeployCTMScript" "${@:3}"
                ;;
            deploy-scripts/DeployErc20.s.sol | deploy-scripts/tokens/DeployErc20.s.sol)
                set -- "$1" "$2:DeployErc20Script" "${@:3}"
                ;;
            deploy-scripts/DeployL1CoreContracts.s.sol | deploy-scripts/ecosystem/DeployL1CoreContracts.s.sol)
                set -- "$1" "$2:DeployL1CoreContractsScript" "${@:3}"
                ;;
            deploy-scripts/DeployL2Contracts.sol | deploy-scripts/chain/DeployL2Contracts.sol)
                set -- "$1" "$2:DeployL2Script" "${@:3}"
                ;;
            deploy-scripts/DeployPaymaster.s.sol | deploy-scripts/chain/DeployPaymaster.s.sol)
                set -- "$1" "$2:DeployPaymaster" "${@:3}"
                ;;
            deploy-scripts/EnableEvmEmulator.s.sol | deploy-scripts/chain/EnableEvmEmulator.s.sol)
                set -- "$1" "$2:EnableEvmEmulator" "${@:3}"
                ;;
            deploy-scripts/RegisterCTM.s.sol | deploy-scripts/ecosystem/RegisterCTM.s.sol)
                set -- "$1" "$2:RegisterCTM" "${@:3}"
                ;;
            deploy-scripts/RegisterZKChain.s.sol | deploy-scripts/ctm/RegisterZKChain.s.sol)
                set -- "$1" "$2:RegisterZKChainScript" "${@:3}"
                ;;
            deploy-scripts/dev/SetupLegacyBridge.s.sol)
                set -- "$1" "$2:SetupLegacyBridge" "${@:3}"
                ;;
        esac
    fi
    exec "$ERA_CACHER_OLD_FOUNDRY_BIN_DIR/TOOL" "$@"
fi

exec "$ERA_CACHER_NEW_FOUNDRY_BIN_DIR/TOOL" "$@"
EOF
        sed -i "s/TOOL/$tool/g" "$dispatch_dir/$tool"
        chmod +x "$dispatch_dir/$tool"
    done

    if [ -x "$new_foundry_bin_dir/anvil" ]; then
        cat > "$dispatch_dir/anvil" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

exec "$ERA_CACHER_NEW_FOUNDRY_BIN_DIR/anvil" "$@"
EOF
        chmod +x "$dispatch_dir/anvil"
    fi

    export ERA_CACHER_WORKING_DIR="$WORKING_DIR"
    export ERA_CACHER_OLD_ZKSYNC_REF="$OLD_ZKSYNC_REF"
    export ERA_CACHER_OLD_FOUNDRY_BIN_DIR="$old_foundry_bin_dir"
    export ERA_CACHER_NEW_FOUNDRY_BIN_DIR="$new_foundry_bin_dir"
    export PATH="$dispatch_dir:$PATH"

    echo "Using v29 forge: $(forge_version_line "$old_foundry_bin_dir/forge")"
    echo "Using v31 forge: $(forge_version_line "$new_foundry_bin_dir/forge")"
}

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

ensure_empty_v31_bridged_tokens_config() {
    local config_path="$WORKING_DIR/contracts/l1-contracts/script-config/v31-bridged-tokens.toml"

    if [ ! -f "$config_path" ]; then
        printf "[tokens]\nbridged_tokens = []\n" > "$config_path"
    fi
}

# ── Phase 0: clean slate, swap to v29 checkout, deploy v29 ecosystem ──────

setup_foundry_dispatch

"$SCRIPT_DIR/reset.sh"

"$SCRIPT_DIR/use-old-era.sh"
cd "$WORKING_DIR"

# We delete information about the gateway chain as presence of old chain configs
# changes the beavior of zkstack ecosystem init.
[ -d "./chains/gateway" ] && rm -rf "./chains/gateway"

# The old v29 checkout's local reth genesis lacks the deterministic CREATE2 factory
# predeploy expected by the v31 deployment scripts. Use the v31 checkout's reth genesis
# (which has the factory) instead of running a manual deploy step from this harness.
rm -rf etc/reth/chaindata
cp -a "$NEW_REPO/etc/reth/chaindata" etc/reth/chaindata

zkstackup --local && zkstack dev clean containers && zkstack up --observability false
wait_for_rpc "$L1_RPC_URL" "l1"

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url="$L1_RPC_URL" \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --verbose \
    --chain era \
    --observability=false

zkstack server --chain era &> "$WORKSPACE_PARENT/rollup3.log" &

wait_for_rpc "$L2_RPC_URL" "pre-upgrade era"

zkstack dev rich-account 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 --chain era

# ── Phase 1: switch to v31 contracts/zkstack, build protocol-ops ──────────

cd "$WORKSPACE_PARENT"
"$SCRIPT_DIR/use-new-era.sh"
cd "$WORKING_DIR"

zkstackup --local
zkstack dev contracts

zkstack dev database migrate --prover false --core true --chain era

# Keep the v29-tree server alive until the onchain upgrade reaches the new ABI.
# The v31-tree runtime cannot start safely against pre-upgrade v29 chain state.

PROTOCOL_OPS_BIN="$WORKING_DIR/contracts/protocol-ops/target/release/protocol_ops"
if [ ! -x "$PROTOCOL_OPS_BIN" ]; then
    (cd "$WORKING_DIR/contracts/protocol-ops" && cargo build --release)
fi

mkdir -p "$PROTOCOL_OPS_WORK"

# ── Phase 2: read shell-only inputs ───────────────────────────────────────
#
# Most addresses (bridgehub, deployer, chain ids, …) are read by
# protocol-ops directly from the zkstack workspace via
# `--zkstack-config-dir`. Only fields that direct `cast`/`forge`
# invocations need are extracted here, with one-liner awks matching the
# prevailing era-cacher style (see neighbouring `update-permanent-values.sh`).
# Wallet PKs other than the deployer never leave the wallets.yaml files —
# protocol-ops consumes them via `--wallets-yaml` for `--execute` dispatch.

BRIDGEHUB_PROXY=$(awk '/^  bridgehub_proxy_addr:/ {print $2; exit}' "$WORKING_DIR/chains/era/configs/contracts.yaml")
DIAMOND_PROXY=$(awk '/^  diamond_proxy_addr:/ {print $2; exit}' "$WORKING_DIR/chains/era/configs/contracts.yaml")
STATE_TRANSITION_PROXY=$(awk '/^  state_transition_proxy_addr:/ {print $2; exit}' "$WORKING_DIR/configs/contracts.yaml")
BYTECODES_SUPPLIER=$(awk '/^  l1_bytecodes_supplier_addr:/ {print $2; exit}' "$WORKING_DIR/configs/contracts.yaml")
ROLLUP_DA_MANAGER=$(awk '/^  l1_rollup_da_manager:/ {print $2; exit}' "$WORKING_DIR/configs/contracts.yaml")
L1_DA_VALIDATOR=$(awk '/^  rollup_l1_da_validator_addr:/ {print $2; exit}' "$WORKING_DIR/chains/era/configs/contracts.yaml")
# DEPLOYER_KEY is only used by `forge script stage3` and the L1 funding
# `cast send` below — neither has a protocol-ops equivalent.
DEPLOYER_KEY=$(awk '/^deployer:/ {flag=1; next} flag && /^  private_key:/ {print $2; exit}' "$WORKING_DIR/configs/wallets.yaml")

# Validate every required field was actually read. Private key checked
# but not echoed (sensitive).
required_addrs=(
    BRIDGEHUB_PROXY DIAMOND_PROXY STATE_TRANSITION_PROXY
    BYTECODES_SUPPLIER ROLLUP_DA_MANAGER L1_DA_VALIDATOR
)
missing=()
for v in "${required_addrs[@]}"; do
    [ -z "${!v:-}" ] && missing+=("$v")
done
[ -z "${DEPLOYER_KEY:-}" ] && missing+=("DEPLOYER_KEY")
if [ "${#missing[@]}" -ne 0 ]; then
    echo "Failed to read required fields from zkstack configs: ${missing[*]}" >&2
    for v in "${required_addrs[@]}"; do echo "  $v=${!v:-<unset>}" >&2; done
    echo "  DEPLOYER_KEY=$([ -n "${DEPLOYER_KEY:-}" ] && echo present || echo missing)" >&2
    exit 1
fi

# Era splits its wallets across two zkstack files: `configs/wallets.yaml`
# (ecosystem deployer + governor) and `chains/era/configs/wallets.yaml`
# (chain governor + operators). Each protocol-ops phase below passes
# `--wallets-yaml` for both; protocol-ops walks both and merges into one
# address → signer map for `--execute`-mode dispatch.
ECO_WALLETS_YAML="$WORKING_DIR/configs/wallets.yaml"
CHAIN_WALLETS_YAML="$WORKING_DIR/chains/era/configs/wallets.yaml"

# ── Phase 3: protocol-ops ecosystem upgrade-prepare (replaces zkstack
#             noGovernancePrepare + ecosystem-admin) ────────────────────────
#
# `--bytecodes-supplier-address` is the *pre-upgrade* address; protocol-ops
# uses it as input to the upgrade and emits a new BytecodesSupplier proxy
# whose address Phase 5 propagates into the runtime config. `--is-zk-sync-os
# false` because this is Era, not OS. `--out` is set so Phase 5 can read
# the upgrade output TOML; `--execute` dispatches the Safe bundle in the
# same call.

GOVERNANCE_TOML="$PROTOCOL_OPS_WORK/governance.toml"
PREPARE_OUT="$PROTOCOL_OPS_WORK/upgrade-prepare"
rm -rf "$PREPARE_OUT" && mkdir -p "$PREPARE_OUT"

echo "=== protocol-ops ecosystem upgrade-prepare ==="
"$PROTOCOL_OPS_BIN" ecosystem upgrade-prepare \
    --l1-rpc-url "$L1_RPC_URL" \
    --zkstack-config-dir "$WORKING_DIR" \
    --bytecodes-supplier-address "$BYTECODES_SUPPLIER" \
    --rollup-da-manager-address "$ROLLUP_DA_MANAGER" \
    --is-zk-sync-os false \
    --governance-toml-out "$GOVERNANCE_TOML" \
    --out "$PREPARE_OUT" \
    --execute \
    --wallets-yaml "$ECO_WALLETS_YAML" \
    --wallets-yaml "$CHAIN_WALLETS_YAML"

# ── Phase 4: protocol-ops ecosystem upgrade-governance (stages 0+1+2 bundled) ─
#
# Single `protocol_ops ecosystem upgrade-governance` invocation runs all
# three governance stages on a forked Anvil, emits one Safe bundle, and
# (with `--execute`) dispatches it. Replaces the four zkstack calls
# (`ecosystem-admin`, `governance-stage0/1/2`) and the
# `yarn upgrade-yaml-output-generator` config-bookkeeping shim. Mirrors
# the OS reference's `run_ecosystem_upgrades` exactly.
#
# Partial-failure note: bundle txs are submitted sequentially and dispatch
# halts on the first revert. Anvil-fork simulation is the primary guard
# against partial application landing on real L1; if state diverges
# between fork and chain we get a hard fail with the reverting tx index,
# not silent partial state.
#
# The `--governance-toml` input was emitted by `upgrade-prepare` via
# `--governance-toml-out`.

echo "=== protocol-ops ecosystem upgrade-governance (stages 0+1+2) ==="
"$PROTOCOL_OPS_BIN" ecosystem upgrade-governance \
    --l1-rpc-url "$L1_RPC_URL" \
    --zkstack-config-dir "$WORKING_DIR" \
    --governance-toml "$GOVERNANCE_TOML" \
    --execute \
    --wallets-yaml "$ECO_WALLETS_YAML" \
    --wallets-yaml "$CHAIN_WALLETS_YAML"

# ── Phase 5: sync runtime contracts.yaml ──────────────────────────────────
#
# `default_ecosystem_upgrade.rs::update_contracts_config_from_output`
# propagates the new `l1_bytecodes_supplier_addr` from the upgrade output
# TOML into the runtime contracts configs. `protocol_ops ecosystem
# sync-runtime-contracts` is the protocol-ops-native sibling: it reads
# `upgrade-prepare`'s emitted TOML and patches every runtime
# `contracts.yaml` under the zkstack workspace before the v31 server
# starts on the new BytecodesSupplier proxy.

echo "=== protocol-ops ecosystem sync-runtime-contracts ==="
"$PROTOCOL_OPS_BIN" ecosystem sync-runtime-contracts \
    --upgrade-output-toml "$WORKING_DIR/contracts/l1-contracts/script-out/v31-upgrade-ecosystem.toml" \
    --zkstack-config-dir "$WORKING_DIR"

# Re-run `zkstack dev contracts` after the sync above. The v31-tree first
# regen on line 117 was for the v29 chain state; now that we've propagated
# the post-upgrade `l1_bytecodes_supplier_addr`, regen the per-chain runtime
# configs the v31 server will read on restart.
zkstack dev contracts

# ── Phase 6: schedule the L2 upgrade timestamp ────────────────────────────
#
# Wall clock NOT `cast block latest --field timestamp`. Reth on
# auto-mining drifts L1 block timestamps ahead of wall clock by tens of
# minutes; using L1 ts puts the upgrade in the future relative to the
# v31 state-keeper's wall clock, so post-restart L2 batches accumulate
# as v29 and `eth_tx_aggregator` filters them out (bootloader-hash
# filter), tries to commit batch N expecting batch 2,
# IncorrectBatchBounds revert, server dies. zkstack's run-chain-upgrade
# uses SystemTime::now() (zkstack_cli/.../args/chain.rs:48); we mirror.

NEW_PROTOCOL_VERSION=$(cast call --rpc-url "$L1_RPC_URL" "$STATE_TRANSITION_PROXY" \
    'protocolVersion()(uint256)' | awk 'NR==1{print $1}')
UPGRADE_TIMESTAMP=$(date -u +%s)

echo "=== protocol-ops chain set-upgrade-timestamp ==="
"$PROTOCOL_OPS_BIN" chain set-upgrade-timestamp \
    --l1-rpc-url "$L1_RPC_URL" \
    --zkstack-config-dir "$WORKING_DIR" \
    --chain era \
    --new-protocol-version "$NEW_PROTOCOL_VERSION" \
    --upgrade-timestamp "$UPGRADE_TIMESTAMP" \
    --execute \
    --wallets-yaml "$ECO_WALLETS_YAML" \
    --wallets-yaml "$CHAIN_WALLETS_YAML"

# ── Phase 7: protocol-ops chain upgrade ───────────────────────────────────

echo "=== protocol-ops chain upgrade ==="
"$PROTOCOL_OPS_BIN" chain upgrade \
    --l1-rpc-url "$L1_RPC_URL" \
    --zkstack-config-dir "$WORKING_DIR" \
    --chain era \
    --execute \
    --wallets-yaml "$ECO_WALLETS_YAML" \
    --wallets-yaml "$CHAIN_WALLETS_YAML"

# ── Phase 7b: verify protocol version on L1 ───────────────────────────────
#
# Honest verification that `chain upgrade` actually moved the chain to v31.
# Mirrors the OS reference's `verify_protocol_version`. SemVer pack format
# (contracts/common/libraries/SemVer.sol):
#   packed = (major << 64) | (minor << 32) | patch
# v31's protocol version is 0.31.0 → packed = 31 << 32 = 133143986176.

echo "=== Verifying protocol version on L1 ==="
# `cast call` prints the decimal repr first then a bracketed scientific
# (e.g. `133143986176 [1.331e11]`); take the first whitespace-delimited
# token. Bash's $(( )) signed 64-bit arith is fine because all three
# fields fit in 32 bits each by construction.
PACKED_VERSION=$(cast call --rpc-url "$L1_RPC_URL" "$DIAMOND_PROXY" \
    'getProtocolVersion()(uint256)' | awk 'NR==1{print $1}')
PROTOCOL_MAJOR=$(( (PACKED_VERSION >> 64) & 0xFFFFFFFF ))
PROTOCOL_MINOR=$(( (PACKED_VERSION >> 32) & 0xFFFFFFFF ))
PROTOCOL_PATCH=$((  PACKED_VERSION        & 0xFFFFFFFF ))
if [ "$PROTOCOL_MAJOR" != "0" ] || [ "$PROTOCOL_MINOR" != "31" ]; then
    echo "Protocol version mismatch: expected 0.31.x, got $PROTOCOL_MAJOR.$PROTOCOL_MINOR.$PROTOCOL_PATCH (packed=$PACKED_VERSION)" >&2
    exit 1
fi
echo "  Protocol version verified: $PROTOCOL_MAJOR.$PROTOCOL_MINOR.$PROTOCOL_PATCH (packed=$PACKED_VERSION)"

# Note (deliberate divergence from OS reference): no wait between chain
# upgrade and the v29→v31 server swap. The OS reference's
# `wait_for_server_to_process_upgrade` works because OS uses *one* server
# binary that straddles the upgrade. Era runs *two* server binaries —
# during chain upgrade, the v29 server's `eth_watch` decodes the v31
# upgrade event, fails to resolve diamond cuts for `protocol version
# 0.31.0`, and crashes. That's expected: the L2 system upgrade tx is
# queued in the L1 priority queue, the v31 server picks it up after the
# Phase 10 swap. Adding a server-side wait here would always time out.

# ── Phase 8: re-set DA validator pair (v31 wipes it) ──────────────────────

echo "=== protocol-ops chain set-da-validator-pair ==="
"$PROTOCOL_OPS_BIN" chain set-da-validator-pair \
    --l1-rpc-url "$L1_RPC_URL" \
    --zkstack-config-dir "$WORKING_DIR" \
    --chain era \
    --l1-da-validator "$L1_DA_VALIDATOR" \
    --l2-da-commitment-scheme blobs-and-pubdata-keccak256 \
    --execute \
    --wallets-yaml "$ECO_WALLETS_YAML" \
    --wallets-yaml "$CHAIN_WALLETS_YAML"

# ── Phase 9: stage3 token migration (no protocol-ops equivalent) ─────────

cd "$WORKING_DIR/contracts/l1-contracts"
ensure_empty_v31_bridged_tokens_config
forge script deploy-scripts/upgrade/v31/EcosystemUpgrade_v31.s.sol:EcosystemUpgrade_v31 \
    --sig "stage3(address)" \
    "$BRIDGEHUB_PROXY" \
    --rpc-url "$L1_RPC_URL" \
    --broadcast \
    --private-key "$DEPLOYER_KEY" \
    --legacy \
    --slow \
    --gas-price 50000000000
cd "$WORKING_DIR"

# ── Phase 10: restart server with v31 tree, run integration tests ────────

pkill -9 zksync_server || true
zkstack server --ignore-prerequisites --chain era &> "$WORKSPACE_PARENT/rollup2.log" &

wait_for_rpc "$L2_RPC_URL" "post-upgrade era" 600

# Fund the main wallet (test_mnemonic index 0) with L1 ETH.
# This wallet is used by init-test-wallet to distribute to the actual test wallet (index 101).
cast send 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 \
    --value 10001ether \
    --rpc-url "$L1_RPC_URL" \
    --private-key "$DEPLOYER_KEY" \
    --gas-price 50gwei

zkstack dev init-test-wallet --chain era
zkstack dev test integration --ignore-prerequisites --chain era \
    api/contract-verification \
    api/debug \
    api/web3 \
    base-token \
    contracts \
    custom-account \
    erc20 \
    ether \
    evm \
    fees \
    interop-a \
    interop-b \
    l1 \
    l2-erc20 \
    mempool \
    paymaster \
    system

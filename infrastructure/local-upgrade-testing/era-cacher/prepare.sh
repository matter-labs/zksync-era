#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

WORKSPACE_DIR="${ERA_CACHER_WORKSPACE_DIR:-$PWD/upgrade-testing}"
OLD_ZKSYNC_REF="${ERA_CACHER_OLD_ZKSYNC_REF:-a273232b3b11c1382209bf52754d91f9dfdb2f17}"
NEW_ZKSYNC_REF="${ERA_CACHER_NEW_ZKSYNC_REF:-$(git -C "$REPO_ROOT" rev-parse HEAD)}"
ZKSYNC_REMOTE="${ERA_CACHER_ZKSYNC_REMOTE:-https://github.com/matter-labs/zksync-era.git}"
NEW_ZKSYNC_REMOTE="${ERA_CACHER_NEW_ZKSYNC_REMOTE:-$REPO_ROOT}"
NEW_CONTRACTS_REF="${ERA_CACHER_CONTRACTS_REF:-$(git -C "$REPO_ROOT" ls-tree HEAD contracts | awk '{print $3}')}"
NEW_CONTRACTS_REMOTE="${ERA_CACHER_CONTRACTS_REMOTE:-https://github.com/matter-labs/era-contracts.git}"
NEW_CONTRACTS_FALLBACK_REMOTE="${ERA_CACHER_CONTRACTS_FALLBACK_REMOTE:-https://github.com/valera-grinenko-ai/era-contracts.git}"

clone_checkout() {
    local name="$1"
    local remote="$2"
    local ref="$3"
    local target="$WORKSPACE_DIR/$name"

    if [ -e "$target" ]; then
        echo "Refusing to overwrite existing $target" >&2
        echo "Remove it first, or set ERA_CACHER_WORKSPACE_DIR to a clean path." >&2
        exit 1
    fi

    echo "Cloning $name from $remote at $ref"
    git clone --no-checkout "$remote" "$target"
    git -C "$target" checkout --detach "$ref"
    git -C "$target" submodule sync --recursive
    git -C "$target" submodule update --init --recursive
}

clone_contracts_checkout() {
    local target="$1"
    local remote

    rm -rf "$target/contracts"
    for remote in "$NEW_CONTRACTS_REMOTE" "$NEW_CONTRACTS_FALLBACK_REMOTE"; do
        echo "Cloning zksync-new/contracts from $remote at $NEW_CONTRACTS_REF"
        if git clone --no-checkout "$remote" "$target/contracts"; then
            if git -C "$target/contracts" checkout --detach "$NEW_CONTRACTS_REF"; then
                git -C "$target/contracts" submodule sync --recursive
                git -C "$target/contracts" submodule update --init --recursive
                return 0
            fi
        fi
        rm -rf "$target/contracts"
    done

    echo "Failed to clone contracts at $NEW_CONTRACTS_REF" >&2
    echo "Set ERA_CACHER_CONTRACTS_REMOTE to a repository containing that commit." >&2
    exit 1
}

clone_new_checkout() {
    local target="$WORKSPACE_DIR/zksync-new"

    if [ -e "$target" ]; then
        echo "Refusing to overwrite existing $target" >&2
        echo "Remove it first, or set ERA_CACHER_WORKSPACE_DIR to a clean path." >&2
        exit 1
    fi

    echo "Cloning zksync-new from $NEW_ZKSYNC_REMOTE at $NEW_ZKSYNC_REF"
    git clone --no-checkout "$NEW_ZKSYNC_REMOTE" "$target"
    git -C "$target" checkout --detach "$NEW_ZKSYNC_REF"
    git -C "$target" submodule sync --recursive
    git -C "$target" submodule update --init --recursive proof-manager-contracts
    clone_contracts_checkout "$target"
}

mkdir -p "$WORKSPACE_DIR"

clone_checkout "zksync-old" "$ZKSYNC_REMOTE" "$OLD_ZKSYNC_REF"
clone_new_checkout

rm -rf "$WORKSPACE_DIR/era-cacher"
cp -a "$SCRIPT_DIR" "$WORKSPACE_DIR/era-cacher"

cat <<EOF

Prepared era-cacher workspace:
  $WORKSPACE_DIR

Checked out:
  zksync-old: $OLD_ZKSYNC_REF
  zksync-new: $NEW_ZKSYNC_REF
  contracts:  $NEW_CONTRACTS_REF

Next:
  cd "$WORKSPACE_DIR"
  bash era-cacher/do-upgrade.sh

If your default foundry-zksync is not v0.1.5 and you do not have the v29
forge/cast binaries in the default locations, set:
  ERA_CACHER_OLD_FOUNDRY_BIN_DIR=/path/to/old/foundry-zksync-ae913af65
  ERA_CACHER_NEW_FOUNDRY_BIN_DIR=/path/to/foundry-zksync-v0.1.5
EOF

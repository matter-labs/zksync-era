#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKING_DIRECTORY="$(cd "$SCRIPT_DIR/../../.." && pwd)"
WORKSPACE_PARENT="$(cd "$WORKING_DIRECTORY/.." && pwd)"
OLD_REPO="$WORKSPACE_PARENT/zksync-old"
NEW_REPO="$WORKSPACE_PARENT/zksync-new"

# Check if the folder exists
if [ ! -d "$NEW_REPO" ]; then
  echo "Error: The folder '$NEW_REPO' does not exist."
  exit 1
else
  echo "Updating to use new era"
fi

rm -rf "$NEW_REPO/chains"
mkdir -p "$NEW_REPO/chains"
cp -rf "$WORKING_DIRECTORY/chains" "$NEW_REPO"


rm -rf "$NEW_REPO/configs"
mkdir -p "$NEW_REPO/configs"
cp -rf "$WORKING_DIRECTORY/configs" "$NEW_REPO"


mv "$WORKING_DIRECTORY" "$OLD_REPO"
mv "$NEW_REPO" "$WORKING_DIRECTORY"

#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKING_DIRECTORY="$(cd "$SCRIPT_DIR/../../.." && pwd)"
WORKSPACE_PARENT="$(cd "$WORKING_DIRECTORY/.." && pwd)"
OLD_REPO="$WORKSPACE_PARENT/zksync-old"
NEW_REPO="$WORKSPACE_PARENT/zksync-new"

if [ -d "$WORKING_DIRECTORY" ] && [ -n "$(ls -A "$WORKING_DIRECTORY")" ]; then
    echo "zksync-working found and is not empty. Cleaning containers and restarting services..."
    cd "$WORKING_DIRECTORY"
    zkstack dev clean containers && zkstack up -o false
fi


if [ -d "$OLD_REPO" ] && [ -z "$(ls -A "$OLD_REPO")" ]; then
    echo "Moving zksync-working to zksync-old"
    mv "$WORKING_DIRECTORY" "$OLD_REPO"
else
    if [ -d "$NEW_REPO" ] && [ -z "$(ls -A "$NEW_REPO")" ]; then
        echo "Moving zksync-working to zksync-new"
        mv "$WORKING_DIRECTORY" "$NEW_REPO"
    else
        echo "Expected an empty sibling checkout at '$OLD_REPO' or '$NEW_REPO'. No reset action taken." >&2
    fi
fi

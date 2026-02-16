#!/bin/bash

OLD_REPO=./zksync-old
NEW_REPO=./zksync-new

WORKING_DIRECTORY=./zksync-working

# If zksync-working exists and zksync-new is empty/doesn't exist, move it to zksync-new
if [ -d "$WORKING_DIRECTORY" ]; then
  if [ ! -d "$NEW_REPO" ] || [ -z "$(ls -A "$NEW_REPO" 2>/dev/null)" ]; then
    echo "Moving existing zksync-working to zksync-new..."
    mv "$WORKING_DIRECTORY" "$NEW_REPO"
  fi
fi

# Check if the folder exists
if [ ! -d "$OLD_REPO" ]; then
  echo "Error: The folder '$OLD_REPO' does not exist."
  exit 1
else
  echo "Updating to use old era."
fi

mv $OLD_REPO $WORKING_DIRECTORY

#!/bin/bash

OLD_REPO=./zksync-old
NEW_REPO=./zksync-new

WORKING_DIRECTORY=./zksync-working

# Check if the folder exists
if [ ! -d "$OLD_REPO" ]; then
  echo "Error: The folder '$OLD_REPO' does not exist."
  exit 1
else
  echo "Updating to use old era."
fi

mv $OLD_REPO $WORKING_DIRECTORY

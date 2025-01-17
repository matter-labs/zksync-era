#!/bin/bash

OLD_REPO=~/zksync-era
NEW_REPO=~/zksync-era-private

WORKING_DIRECTORY=~/zksync-era-current

# Check if the folder exists
if [ ! -d "$OLD_REPO" ]; then
  echo "Error: The folder '$OLD_REPO' does not exist."
  exit 1
else
  echo "Updating to use old era."
fi

mv $OLD_REPO $WORKING_DIRECTORY

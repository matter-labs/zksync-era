#!/bin/bash

OLD_REPO=~/zksync-era
NEW_REPO=~/zksync-era-private

WORKING_DIRECTORY=~/zksync-era-current

# Check if the folder exists
if [ ! -d "$NEW_REPO" ]; then
  echo "Error: The folder '$NEW_REPO' does not exist."
  exit 1
else
  echo "Updating to use new era"
fi

rm -rf $NEW_REPO/chains
mkdir $NEW_REPO/chains
cp -rf $WORKING_DIRECTORY/chains $NEW_REPO


rm -rf $NEW_REPO/configs
mkdir $NEW_REPO/configs
cp -rf $WORKING_DIRECTORY/configs $NEW_REPO


mv $WORKING_DIRECTORY $OLD_REPO
mv $NEW_REPO $WORKING_DIRECTORY

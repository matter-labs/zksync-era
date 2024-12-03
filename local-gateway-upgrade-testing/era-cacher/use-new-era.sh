#!/bin/bash

# Specify the folder path
FOLDER_PATH=~/zksync-era-new

# Check if the folder exists
if [ ! -d "$FOLDER_PATH" ]; then
  echo "Error: The folder '$FOLDER_PATH' does not exist."
  exit 1
else
  echo "Updating to use new era"
fi

rm -rf ~/zksync-era-new/chains
mkdir ~/zksync-era-new/chains
cp -rf ~/zksync-era/chains ~/zksync-era-new


rm -rf ~/zksync-era-new/configs
mkdir ~/zksync-era-new/configs
cp -rf ~/zksync-era/configs ~/zksync-era-new


mv ~/zksync-era ~/zksync-era-old
mv ~/zksync-era-new ~/zksync-era

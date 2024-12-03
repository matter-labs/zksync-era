#!/bin/bash

# Specify the folder path
FOLDER_PATH=~/zksync-era-old

# Check if the folder exists
if [ ! -d "$FOLDER_PATH" ]; then
  echo "Error: The folder '$FOLDER_PATH' does not exist."
  exit 1
else
  echo "Updating to use old era."
fi


mv ~/zksync-era ~/zksync-era-new
mv ~/zksync-era-old ~/zksync-era

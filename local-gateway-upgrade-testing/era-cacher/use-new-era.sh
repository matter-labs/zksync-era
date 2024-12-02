#!/bin/bash

rm -rf ~/zksync-era-new/chains
mkdir ~/zksync-era-new/chains
cp -rf ~/zksync-era/chains ~/zksync-era-new


rm -rf ~/zksync-era-new/configs
mkdir ~/zksync-era-new/configs
cp -rf ~/zksync-era/configs ~/zksync-era-new

mv ~/zksync-era ~/zksync-era-old
mv ~/zksync-era-new ~/zksync-era

#!/bin/bash
# to use this properly comment out the section that fails the node if the bootloader has is wrong in genesis/src/lib.rs


zkstack dev clean containers && zkstack up -o false
zkstack dev contracts

zkstack dev generate-genesis

zkstack ecosystem init --dev --observability=false \
    --update-submodules false

zkstack chain convert-to-gateway --chain gateway --ignore-prerequisites
zkstack server --ignore-prerequisites --chain gateway &> ./gateway.log & 

zkstack server wait --ignore-prerequisites --verbose --chain gateway
zkstack chain migrate-to-gateway --chain era --gateway-chain-name gateway
# zkstack chain migrate-to-gateway --chain second --gateway-chain-name gateway

 zkstack server --ignore-prerequisites --chain era &> ./rollup.log &

#  zkstack server --ignore-prerequisites --chain second &> ./second.log &

# sleep 20

# zkstack dev test integration -t "L1 ERC20" --no-deps --ignore-prerequisites --chain era

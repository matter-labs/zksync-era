#!/bin/bash
# to use this properly comment out the section that fails the node if the bootloader has is wrong in genesis/src/lib.rs


zkstack dev clean containers && zkstack up -o false
zkstack dev contracts

zkstack dev generate-genesis

zkstack ecosystem init --dev --observability=false \
    --update-submodules false



 zkstack server --ignore-prerequisites --chain era &> ./rollup.log &

 zkstack server --ignore-prerequisites --chain second &> ./second.log &

# sleep 20

# zkstack dev test integration -t "Interop" --no-deps --ignore-prerequisites --chain era

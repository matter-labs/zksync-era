#!/bin/bash
# to use this properly comment out the section that fails the node if the bootloader has is wrong in genesis/src/lib.rs


sudo zkstack dev clean containers && zkstack up -o false

output=$( zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
            --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_era \
            --ignore-prerequisites --observability=false \
            --chain era \
            --update-submodules false 2>&1)
            # --no-port-reallocation 
        #     --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
        #     --prover-db-name=zksync_prover_localhost_era \
            # --skip-contract-compilation-override \

# Extract the values using grep and awk
bootloader_hash=$(echo "$output" | grep "bootloader_hash:" | awk -F': ' '{print $2}' | xargs)
default_aa_hash=$(echo "$output" | grep "default_aa_hash:" | awk -F': ' '{print $2}' | xargs)


# Ensure all values were found
if [[ -n "$bootloader_hash" && -n "$default_aa_hash" ]]; then
    echo "Updating genesis.yaml with new values."
    echo "bootloader_hash: $bootloader_hash"
    echo "default_aa_hash: $default_aa_hash"

    # echo "$output"

    # Update genesis.yaml using sed
    # sed -i "s/^bootloader_hash: .*/bootloader_hash: \"$bootloader_hash\"/" "/Users/kalmanlajko/programming/zksync/tmp2/zksync-era/etc/env/file_based/genesis.yaml"
    perl -pi -e "s/^bootloader_hash:.*/bootloader_hash: $bootloader_hash/" "./etc/env/file_based/genesis.yaml"
    perl -pi -e "s/^default_aa_hash:.*/default_aa_hash: $default_aa_hash/" "./etc/env/file_based/genesis.yaml"


    echo "Updated genesis.yaml with new values."

    output=$( zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
            --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_era \
            --ignore-prerequisites --observability=false \
            --chain era \
            --update-submodules false 2>&1)

fi

genesis_root=$(echo "$output" | grep "genesis_root:" | awk -F': ' '{print $2}' | xargs)
genesis_batch_commitment=$(echo "$output" | grep "genesis_batch_commitment:" | awk -F': ' '{print $2}' | xargs)

# we need an if here, as the genesis_root depends on the bootloader_hash 
if [[ -n "$genesis_root" && -n "$genesis_batch_commitment" ]]; then
    echo "Updating genesis.yaml 2 with new values."
    echo "genesis_root: $genesis_root"
    echo "genesis_batch_commitment: $genesis_batch_commitment"
    perl -pi -e "s/^genesis_root:.*/genesis_root: $genesis_root/" "./etc/env/file_based/genesis.yaml"
    perl -pi -e "s/^genesis_batch_commitment:.*/genesis_batch_commitment: $genesis_batch_commitment/" "./etc/env/file_based/genesis.yaml"

    zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
        --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
        --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
        --server-db-name=zksync_server_localhost_era \
        --ignore-prerequisites --observability=false \
        --chain era \
        --update-submodules false
else
    echo "Update of genesis.yaml was not needed."
    echo "$output"
fi

zkstack chain create \
        --chain-name second \
        --chain-id 505 \
        --prover-mode no-proofs \
        --wallet-creation localhost \
        --l1-batch-commit-data-generator-mode rollup \
        --base-token-address 0x0000000000000000000000000000000000000001 \
        --base-token-price-nominator 1 \
        --base-token-price-denominator 1 \
        --set-as-default false \
        --evm-emulator false \
        --ignore-prerequisites --update-submodules false 
        # --skip-contract-compilation-override

zkstack chain init \
            --deploy-paymaster \
            --l1-rpc-url=http://localhost:8545 \
            --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
            --server-db-name=zksync_server_localhost_gateway \
            --chain second --update-submodules false --no-port-reallocation
        #     --prover-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
        #     --prover-db-name=zksync_prover_localhost_gateway \

# zkstack server --ignore-prerequisites --chain era &> ./rollup.log & 

# zkstack server --ignore-prerequisites --chain second &> ./second.log & 

# sleep 20

# zkstack dev test integration -t "Interop" --no-deps --ignore-prerequisites --chain era

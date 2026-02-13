era-cacher/reset.sh

era-cacher/use-old-era.sh && cd zksync-working

upgrade_version="v31-interop-b"
upgrade_file_extension="v31"

# We delete information about the gateway chain as presence of old chain configs
# changes the beavior of zkstack ecosystem init.
[ -d "./chains/gateway" ] && rm -rf "./chains/gateway"

zkstackup --local && zkstack dev clean containers && zkstack up --observability false

# zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
#     --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
#     --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
#     --server-db-name=zksync_server_localhost_era \
#     --ignore-prerequisites --verbose \
#     --chain era \
#     --observability=false

# zkstack dev generate-genesis 

zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
    --deploy-ecosystem --l1-rpc-url=http://127.0.0.1:8545 \
    --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
    --server-db-name=zksync_server_localhost_era \
    --ignore-prerequisites --verbose \
    --chain era \
    --observability=false

zkstack server --chain era &> ../rollup3.log &

zkstack dev rich-account 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049  --chain era 

sleep 10

pkill -9 zksync_server

# zkstack dev generate-genesis 

# zkstack chain create \
#     --chain-name gateway \
#     --chain-id 505 \
#     --prover-mode no-proofs \
#     --wallet-creation localhost \
#     --l1-batch-commit-data-generator-mode rollup \
#     --base-token-address 0x0000000000000000000000000000000000000001 \
#     --base-token-price-nominator 1 \
#     --base-token-price-denominator 1 \
#     --set-as-default false \
#     --evm-emulator false \
#     --ignore-prerequisites

# zkstack chain init \
#     --deploy-paymaster \
#     --l1-rpc-url=http://127.0.0.1:8545 \
#     --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
#     --server-db-name=zksync_server_localhost_gateway \
#     --chain gateway

# zkstack chain gateway convert-to-gateway --chain gateway
# # When running locally, it makes sense to not redirect to file, but start a new terminal window here.
# zkstack server --chain gateway &> ../gateway.log &

# # When running locally, open a new terminal window here.
# zkstack chain gateway migrate-to-gateway --chain era --gateway-chain-name gateway
# # When running locally, it makes sense to not redirect to file, but start a new terminal window during the next operation.
# zkstack server --chain era &> ../rollup3.log &

# When running locally, open a new terminal window here.
cd .. && era-cacher/use-new-era.sh && cd zksync-working

# If you are running locally, ensure to go out and back to the zksync-working directory, i.e.
# `cd ..` and then `cd zksync-working`.
# Sometimes, the console may not reflect the changes made to the codebase.

zkstackup --local
zkstack dev contracts

# cd contracts
# git checkout
# cd ..

zkstack dev database migrate --prover false --core true  --chain era
# zkstack dev database migrate --prover false --core true  --chain gateway

# All the actions below may be performed in a different window.
# zkstack server --ignore-prerequisites --chain gateway &> ../gateway.log &
RUST_BACKTRACE=1 zkstack server --ignore-prerequisites --chain era &> ../rollup.log &

echo "Server started"

# Wait for server to be ready
sleep 5

# Update permanent-values.toml with addresses from running deployment
../era-cacher/update-permanent-values.sh

zkstack dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage no-governance-prepare
zkstack dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage ecosystem-admin
zkstack dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage governance-stage0
zkstack dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage governance-stage1

cd contracts/l1-contracts
UPGRADE_ECOSYSTEM_OUTPUT=script-out/v31-upgrade-ecosystem.toml UPGRADE_ECOSYSTEM_OUTPUT_TRANSACTIONS=broadcast/EcosystemUpgrade_v31.s.sol/9/run-latest.json  YAML_OUTPUT_FILE=script-out/v31-local-output.yaml yarn upgrade-yaml-output-generator
cd ../../

zkstack dev run-chain-upgrade --upgrade-version $upgrade_version --force-display-finalization-params=true --dangerous-local-default-overrides=true --chain era
# zkstack dev run-chain-upgrade --upgrade-version $upgrade_version --force-display-finalization-params=true --dangerous-local-default-overrides=true --chain gateway
zkstack dev run-ecosystem-upgrade --upgrade-version $upgrade_version --ecosystem-upgrade-stage governance-stage2

# Stage 3: Migrate token balances from NTV to AssetTracker
# This can be done with any private key (deployer is used here)
cd contracts/l1-contracts
forge script deploy-scripts/upgrade/v31/EcosystemUpgrade_v31.s.sol:EcosystemUpgrade_v31 \
    --sig "stage3()" \
    --rpc-url http://localhost:8545 \
    --broadcast \
    --private-key 0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 \
    --legacy \
    --slow \
    --gas-price 50000000000
cd ../../

pkill -9 zksync_server
zkstack server --ignore-prerequisites --chain era &> ../rollup2.log &

sleep 10

# Fund the main wallet (test_mnemonic index 0) with L1 ETH
# This wallet is used by init-test-wallet to distribute to the actual test wallet (index 101)
# init-test-wallet sends 10k ETH, so we need at least that much plus gas
# Using the rich L1 account (RETH pre-funded account) to send ETH
cast send 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 \
    --value 10001ether \
    --rpc-url http://127.0.0.1:8545 \
    --private-key 0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 \
    --gas-price 50gwei

# Initialize test wallet - this will distribute 10k ETH from main wallet to test wallet
zkstack dev init-test-wallet --chain era

zkstack dev test integration --no-deps --ignore-prerequisites --chain era

#!/bin/bash
# Update permanent-values.toml with addresses from running server and zkstack config

set -e

echo "Updating permanent-values.toml with deployed contract addresses..."

# Change to zksync-working directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKING_DIR="$SCRIPT_DIR/../zksync-working"

if [ ! -d "$WORKING_DIR" ]; then
  echo "Error: zksync-working directory not found at $WORKING_DIR"
  exit 1
fi

cd "$WORKING_DIR"
echo "Working directory: $(pwd)"

# Get RPC URL from zkstack config
RPC_URL=$(awk '/http_url:/ {print $2; exit}' chains/era/configs/general.yaml)

if [ -z "$RPC_URL" ]; then
  echo "Error: Failed to read RPC URL from chains/era/configs/general.yaml"
  exit 1
fi

echo "Using RPC URL: $RPC_URL"

# Get bridgehub address from contracts.yaml (ecosystem_contracts section)
BRIDGEHUB_ADDR=$(grep -A 20 "^ecosystem_contracts:" chains/era/configs/contracts.yaml | grep "bridgehub_proxy_addr:" | awk '{print $2}' | head -1)

if [ -z "$BRIDGEHUB_ADDR" ]; then
  echo "Error: Failed to read bridgehub_proxy_addr from ecosystem_contracts in chains/era/configs/contracts.yaml"
  exit 1
fi

echo "Bridgehub address (from config): $BRIDGEHUB_ADDR"

# Read other addresses from the canonical zkstack config.
# The generated per-chain config can lag behind post-upgrade address updates.
ERA_CHAIN_ID=$(awk '/^  l2_chain_id:/ {print $2; exit}' chains/era/configs/general.yaml)
CTM_ADDR=$(awk '/state_transition_proxy_addr:/ {print $2; exit}' configs/contracts.yaml)
BYTECODES_SUPPLIER=$(awk '/l1_bytecodes_supplier_addr:/ {print $2; exit}' configs/contracts.yaml)
CREATE2_FACTORY=$(awk '/^create2_factory_addr:/ {print $2; exit}' configs/contracts.yaml)
CREATE2_SALT=$(awk '/^create2_factory_salt:/ {print $2; exit}' configs/contracts.yaml)

# Validate all required values were read
if [ -z "$ERA_CHAIN_ID" ]; then
  echo "Error: Failed to read l2_chain_id from chains/era/configs/general.yaml"
  exit 1
fi

if [ -z "$CTM_ADDR" ]; then
  echo "Error: Failed to read state_transition_proxy_addr from configs/contracts.yaml"
  exit 1
fi

if [ -z "$BYTECODES_SUPPLIER" ]; then
  echo "Error: Failed to read l1_bytecodes_supplier_addr from configs/contracts.yaml"
  exit 1
fi

echo "ERA Chain ID: $ERA_CHAIN_ID"
echo "CTM Address: $CTM_ADDR"
echo "Bytecodes Supplier: $BYTECODES_SUPPLIER"

# Update permanent-values.toml in both locations
OUTPUT_FILE1="contracts/l1-contracts/script-config/permanent-values.toml"
OUTPUT_FILE2="contracts/l1-contracts/upgrade-envs/permanent-values/local.toml"
CHAIN_CONTRACTS_FILE="chains/era/configs/contracts.yaml"

# Create the directory if it doesn't exist
mkdir -p "$(dirname "$OUTPUT_FILE2")"

# Generate the content
CONTENT=$(cat <<EOF
era_chain_id = $ERA_CHAIN_ID

[core_contracts]
bridgehub_proxy_addr = "$BRIDGEHUB_ADDR"

[ctm_contracts]
ctm_proxy_addr = "$CTM_ADDR"
l1_bytecodes_supplier_addr = "$BYTECODES_SUPPLIER"

[permanent_contracts]
create2_factory_addr = "$CREATE2_FACTORY"
create2_factory_salt = "$CREATE2_SALT"
EOF
)

# Write to both files
echo "$CONTENT" > "$OUTPUT_FILE1"
echo "$CONTENT" > "$OUTPUT_FILE2"

# The runtime reads the generated per-chain contracts config directly.
# Keep the key addresses in sync with the canonical config used above.
if [ -f "$CHAIN_CONTRACTS_FILE" ]; then
  sed -i -E "s#(^  bridgehub_proxy_addr: ).*#\\1$BRIDGEHUB_ADDR#" "$CHAIN_CONTRACTS_FILE"
  sed -i -E "s#(^  state_transition_proxy_addr: ).*#\\1$CTM_ADDR#" "$CHAIN_CONTRACTS_FILE"
  sed -i -E "s#(^  l1_bytecodes_supplier_addr: ).*#\\1$BYTECODES_SUPPLIER#" "$CHAIN_CONTRACTS_FILE"
fi

echo ""
echo "✓ Updated $OUTPUT_FILE1"
echo "✓ Updated $OUTPUT_FILE2"
echo "✓ Synced $CHAIN_CONTRACTS_FILE"
echo ""
echo "  - ERA chain ID: $ERA_CHAIN_ID"
echo "  - Bridgehub: $BRIDGEHUB_ADDR"
echo "  - CTM: $CTM_ADDR"
echo "  - Bytecodes Supplier: $BYTECODES_SUPPLIER"

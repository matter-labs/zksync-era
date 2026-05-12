#!/bin/bash
# Debug a failed transaction by sending it with high gas and getting the trace
# Usage: ./debug-tx.sh <to_address> <calldata> [value_in_wei]

set -e

TO_ADDRESS="$1"
CALLDATA="$2"
VALUE="${3:-0}"

if [ -z "$TO_ADDRESS" ] || [ -z "$CALLDATA" ]; then
    echo "Usage: $0 <to_address> <calldata> [value_in_wei]"
    echo "Example: $0 0xfe3EE966E7790b427F7B078f304C7B4DDCd4bbfe 0xd52471c1... 1050000121535147500000"
    exit 1
fi

RPC_URL="${RPC_URL:-http://127.0.0.1:8545}"
PRIVATE_KEY="${PRIVATE_KEY:-0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110}"

echo "==================================="
echo "Sending transaction..."
echo "To: $TO_ADDRESS"
echo "Value: $VALUE wei"
echo "Calldata: ${CALLDATA:0:66}..."
echo "==================================="

# Send transaction with high gas limit
TX_HASH=$(cast send "$TO_ADDRESS" \
  --data "$CALLDATA" \
  --value "$VALUE" \
  --private-key "$PRIVATE_KEY" \
  --rpc-url "$RPC_URL" \
  --gas-limit 10000000 \
  --gas-price 50gwei 2>&1 | grep "transactionHash" | awk '{print $2}')

echo ""
echo "Transaction hash: $TX_HASH"
echo ""
echo "==================================="
echo "Getting trace..."
echo "==================================="
echo ""

# Get the trace
cast run "$TX_HASH" --rpc-url "$RPC_URL"

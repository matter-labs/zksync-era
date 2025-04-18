#!/bin/bash

# Colors for the terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

HELP=$(cat <<'HELP_EOF'
Usage: run_on_all_chains.sh CMD CHAINS LOG_DIR [...CHAIN_ARGS]
  Runs CMD on CHAINS (comma-separated) redirecting stdout + stderr of each command to a file in LOG_DIR.
  CHAIN_ARGS allow specifying additional chain-specific args; they are encoded as `$chain:$args`.

Examples:
  run_on_all_chains.sh 'zkstack dev test integration' era,validium logs 'era:--evm'
    This will run integration tests for 'era' and 'validium' chains, with an additional '--evm' flag
    passed just to the 'era' chain.
HELP_EOF
)

if [ "$1" == "--help" ]; then
    echo "$HELP"
    exit 0
elif [ "$#" -lt 3 ]; then
    echo -e "${RED}Error:${NC} expected at least 3 args"
    echo "$HELP"
    exit 2
fi

command=$1
chain_list=$2
log_dir=$3
IFS=',' read -r -a chains <<< "$chain_list"
pids=()
statuses=()

# Read chain-specific args
shift 3
additional_chain_args=()
for arg in "$@"; do
    for i in "${!chains[@]}"; do
        if [ "${chains[$i]}" == "${arg/:*/}" ]; then
            additional_chain_args[$i]="${arg/*:/}"
        fi
    done
done

# Start background processes
for i in "${!chains[@]}"; do
    full_command="$command ${additional_chain_args[$i]} --chain ${chains[$i]} &> ${log_dir}/${chains[$i]}.log"
    echo "Running command: $full_command"
    eval "$full_command" &
    pids+=($!)
done

# Wait for all processes to complete and capture their exit statuses
for i in "${!pids[@]}"; do
    wait ${pids[$i]}
    statuses[$i]=$?
done

# Check exit statuses and set overall status
overall_status=0

for i in "${!statuses[@]}"; do
    if [ ${statuses[$i]} -ne 0 ]; then
        overall_status=1
        echo -e "${RED}✗ ERROR (exit code ${statuses[$i]}): ${chains[$i]}${NC}"
    else
        echo -e "${GREEN}✓ SUCCESS: ${chains[$i]}${NC}"
    fi
done

# Exit with overall status
exit $overall_status

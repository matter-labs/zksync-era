#!/bin/bash

# Colors for the terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color


command=$1
chain_list=$2
log_dir=$3
IFS=',' read -r -a chains <<< "$chain_list"
pids=()
statuses=()

# Start background processes
for i in "${!chains[@]}"; do
    eval "$command --chain ${chains[$i]} &> ${log_dir}/${chains[$i]}.log" &
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

#! /bin/bash

# Disable immediate exit on non-zero status
set +e

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
        echo "Chain ${chains[$i]} failed with status ${statuses[$i]}"
    else
        echo "Chain ${chains[$i]} succeeded"
    fi
done

# Re-enable immediate exit on non-zero status
set -e

# Exit with overall status
exit $overall_status

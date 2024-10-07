#! /bin/bash

run_on_all_chains() {
    # Disable immediate exit on non-zero status
    set +e

    local command=$1
    local chains=("era" "validium" "custom_token" "consensus")
    local logs=(${{ env.INTEGRATION_TESTS_LOGS_DIR }} ${{ env.INTEGRATION_TESTS_LOGS_DIR }} ${{ env.INTEGRATION_TESTS_LOGS_DIR }} ${{ env.INTEGRATION_TESTS_LOGS_DIR }})
    local pids=()
    local statuses=()

    # Start background processes
    for i in "${!chains[@]}"; do
        eval "$command --chain ${chains[$i]} &> ${logs[$i]}/${chains[$i]}.log" &
        pids+=($!)
    done

    # Wait for all processes to complete and capture their exit statuses
    for i in "${!pids[@]}"; do
        wait ${pids[$i]}
        statuses[$i]=$?
    done

    # Check exit statuses and set overall status
    local overall_status=0

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
    return $overall_status
}

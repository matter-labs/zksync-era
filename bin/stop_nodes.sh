#!/bin/bash

set -e

GRACEFUL_TIMEOUT=10 # seconds

HELP=$(cat <<'HELP_EOF'
Usage: stop_nodes.sh [--en]
  Stops all node processes (`zksync_server`), first gracefully, then killing on a timeout.
  If the '--en' flag is specified, stops external nodes (`zksync_external_node`) instead.
HELP_EOF
)

if [ "$1" == "--help" ]; then
    echo "$HELP"
    exit 0
fi

PGREP_ARGS=''
if [ "$(uname -s)" == "Linux" ]; then
    PGREP_ARGS='-r RS' # Only find running and sleeping processes
fi

function stop_processes {
    echo "Stopping $1 node processes..."
    killall -INT "$1" || return 0 # No matching processes; this is fine

    # We don't want to use Linux-specific `timeout` command or `killall --wait` extension
    elapsed=0
    pids=""
    while [ $elapsed -le $GRACEFUL_TIMEOUT ]; do
        pids=$(pgrep -d, -f $PGREP_ARGS "$1" || true)
        if [ "$pids" == "" ]; then
            echo "All $1 processes have stopped after ${elapsed}s"
            return 0
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done

    echo "Some $1 processes are not stopped after ${GRACEFUL_TIMEOUT}s:"
    # Output command-line args for the remaining processes, so that it's easier to debug their hang-up reason.
    ps -o pid,state,command -p "$pids" || true
    echo "Killing remaining $1 processes"
    killall -KILL "$1" || true
}

if [ "$1" == "--en" ]; then
    stop_processes zksync_external_node
else
    stop_processes zksync_server
fi

#!/bin/bash

set -o errexit
set -o nounset
set -o xtrace

echo "Running $INTEGRATION_TEST_NODE_BINARY_PATH"
exec $INTEGRATION_TEST_NODE_BINARY_PATH

#!/bin/bash

gpu_file="setup-data-gpu-keys.json"

# Process GPU file
value=$(jq -r '.us' "./prover/$gpu_file")
short_sha=$(echo $value | sed 's|gs://matterlabs-setup-data-us/\(.*\)/|\1|')
echo "gpu_short_commit_sha=$short_sha"

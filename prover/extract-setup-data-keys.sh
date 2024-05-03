#!/bin/bash

# Define file names
cpu_file="setup-data-cpu-keys.json"
gpu_file="setup-data-gpu-keys.json"

# Process CPU file
value=$(jq -r '.us' "./prover/$cpu_file")
short_sha=$(echo $value | sed 's|gs://matterlabs-setup-data-us/\(.*\)/|\1|')
echo "cpu_short_commit_sha=$short_sha"

# Process GPU file
value=$(jq -r '.us' "./prover/$gpu_file")
short_sha=$(echo $value | sed 's|gs://matterlabs-setup-data-us/\(.*\)/|\1|')
echo "gpu_short_commit_sha=$short_sha"

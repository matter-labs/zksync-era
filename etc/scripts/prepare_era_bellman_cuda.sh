#!/bin/bash
set -e

source="$1"
binary="$2"

curl --silent --location "${source}" --output bellman-cuda-source.tar.gz
curl --silent --location "${binary}" --output bellman-cuda.tar.gz
mkdir -p bellman-cuda
tar xvfz bellman-cuda.tar.gz -C ./bellman-cuda
tar xvfz bellman-cuda-source.tar.gz -C ./bellman-cuda --strip-components=1

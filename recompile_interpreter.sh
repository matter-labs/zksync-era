#!/usr/bin/env bash
set -e

zksolc contracts/system-contracts/contracts/EvmInterpreter.yul --optimization 3 --yul --bin --overwrite -o contracts/system-contracts/contracts-preprocessed/artifacts/

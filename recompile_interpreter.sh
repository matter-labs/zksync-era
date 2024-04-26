#!/usr/bin/env bash
set -e

preprocess -f contracts/system-contracts/contracts/EvmInterpreter.template.yul -d contracts/system-contracts/contracts/EvmInterpreterPreprocessed.yul
zksolc contracts/system-contracts/contracts/EvmInterpreterPreprocessed.yul --optimization 3 --jump-table-density-threshold 5 --yul --bin --overwrite -o contracts/system-contracts/contracts-preprocessed/artifacts/
python hex_to_binary.py
mv -f bytecode contracts/system-contracts/contracts-preprocessed/artifacts/EvmInterpreterPreprocessed.yul.zbin

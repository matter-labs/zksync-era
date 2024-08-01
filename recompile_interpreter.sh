#!/usr/bin/env bash
set -e

VERSION_OUTPUT=$(zksolc --version)
if [[ $VERSION_OUTPUT == *"1.4.1"* ]]; then
    JUMP_TABLE_FLAG="--jump-table-density-threshold 5"
else
    JUMP_TABLE_FLAG=
fi

preprocess -f contracts/system-contracts/contracts/EvmInterpreter.template.yul -d contracts/system-contracts/contracts/EvmInterpreterPreprocessed.yul
zksolc contracts/system-contracts/contracts/EvmInterpreterPreprocessed.yul --optimization 3 $JUMP_TABLE_FLAG --yul --bin --overwrite -o contracts/system-contracts/contracts-preprocessed/artifacts/

VERSION_OUTPUT=$(zksolc --version)
if [[ $VERSION_OUTPUT == *"1.5.0"* ]]; then
    mv -f contracts/system-contracts/contracts-preprocessed/artifacts/contracts/EvmInterpreterPreprocessed.yul.zbin contracts/system-contracts/contracts-preprocessed/artifacts/EvmInterpreterPreprocessed.yul.zbin
fi

VERSION_OUTPUT=$(zksolc --version)
if [[ $VERSION_OUTPUT == *"1.4.1"* ||  $VERSION_OUTPUT == *"1.5.0"* ]]; then
    python3 hex_to_binary.py
    mv -f bytecode contracts/system-contracts/contracts-preprocessed/artifacts/EvmInterpreterPreprocessed.yul.zbin
fi

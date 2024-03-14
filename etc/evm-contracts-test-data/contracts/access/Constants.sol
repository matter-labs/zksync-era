// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

import "./interfaces/IEvmGasManager.sol";

uint160 constant SYSTEM_CONTRACTS_OFFSET = 0x8000;

IEvmGasManager constant EVM_GAS_MANAGER = IEvmGasManager(address(SYSTEM_CONTRACTS_OFFSET + 0x14));

uint256 constant INF_PASS_GAS = 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff;
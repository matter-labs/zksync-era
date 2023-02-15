// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

import "./interfaces/INonceHolder.sol";
import "./interfaces/IContractDeployer.sol";
import "./SystemContext.sol";

uint160 constant SYSTEM_CONTRACTS_OFFSET = 0x8000; // 2^15

address constant ECRECOVER_SYSTEM_CONTRACT = address(0x01);
address constant SHA256_SYSTEM_CONTRACT = address(0x02);

address payable constant BOOTLOADER_FORMAL_ADDRESS = payable(address(SYSTEM_CONTRACTS_OFFSET + 0x01));
INonceHolder constant NONCE_HOLDER_SYSTEM_CONTRACT = INonceHolder(address(SYSTEM_CONTRACTS_OFFSET + 0x03));

// A contract that is allowed to deploy any codehash
// on any address. To be used only during an upgrade.
address constant FORCE_DEPLOYER = address(SYSTEM_CONTRACTS_OFFSET + 0x07);
address constant MSG_VALUE_SYSTEM_CONTRACT = address(SYSTEM_CONTRACTS_OFFSET + 0x09);
IContractDeployer constant DEPLOYER_SYSTEM_CONTRACT = IContractDeployer(address(SYSTEM_CONTRACTS_OFFSET + 0x06));


address constant KECCAK256_SYSTEM_CONTRACT = address(SYSTEM_CONTRACTS_OFFSET + 0x10);

address constant ETH_TOKEN_SYSTEM_CONTRACT = address(SYSTEM_CONTRACTS_OFFSET + 0x0a);
SystemContext constant SYSTEM_CONTEXT_CONTRACT = SystemContext(address(SYSTEM_CONTRACTS_OFFSET + 0x0b));

uint256 constant MAX_SYSTEM_CONTRACT_ADDRESS = 0xffff;

bytes32 constant DEFAULT_ACCOUNT_CODE_HASH = 0x00;

// The number of bytes that are published during the contract deployment
// in addition to the bytecode itself.
uint256 constant BYTECODE_PUBLISHING_OVERHEAD = 100;

uint256 constant MSG_VALUE_SIMULATOR_IS_SYSTEM_BIT = 2**128;

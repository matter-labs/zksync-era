// SPDX-License-Identifier: UNLICENSED

// This contract is used to estimate the protocol properties
// related to the fee calculation, such as block capacity
// and different operations costs.

pragma solidity ^0.8.0;

// Copied from `contracts/zksync/contracts/L2ContractHelper.sol`.
interface IL2Messenger {
    function sendToL1(bytes memory _message) external returns (bytes32);
}

uint160 constant SYSTEM_CONTRACTS_OFFSET = 0x8000; // 2^15
IL2Messenger constant L2_MESSENGER = IL2Messenger(address(SYSTEM_CONTRACTS_OFFSET + 0x08));

// TODO: Should be set to the actual value (SMA-1185).
// Represents the maximum amount of L2->L1 messages that can happen in one block.
uint256 constant MAX_L2_L1_MESSAGES_IN_BLOCK = 256;

contract Estimator {
    function estimateBlockCapacity() public {
        // Block capacity is defined by several parameters, but the "cheapest" way to seal the block
        // is to send a limited amount of messages to the L1.
        // Here we're going to do just it.
        for (uint256 i = 0; i < MAX_L2_L1_MESSAGES_IN_BLOCK; i++) {
            L2_MESSENGER.sendToL1(bytes(""));
        }
    }
}

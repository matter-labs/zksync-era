// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract CounterFallback {
    // Fallback function
    fallback() external payable {
        // Implement any logic you want the contract to perform when it receives Ether
        // This function will be called when the contract receives Ether and no other function matches the call data
        uint256 value = 0;
        value += 1;
    }
}

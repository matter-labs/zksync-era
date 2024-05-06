// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract CounterFallback {

    function performCall() external {
        uint256 value = 0;
        value += 1;
    }

    fallback() external {
        this.performCall();
    }
}

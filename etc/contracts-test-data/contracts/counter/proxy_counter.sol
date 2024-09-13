// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

interface ICounter {
    function increment(uint256 x) external;
}

contract ProxyCounter {
    ICounter counter;

    constructor(ICounter _counter) {
        counter = _counter;
    }

    uint256 lastFarCallCost;

    function increment(uint256 x, uint gasToPass) public {
        while (gasleft() > gasToPass) {
            // Burn gas so that there's about `gasToPass` left before the external call.
        }
        uint256 gasBefore = gasleft();
        counter.increment(x);
        lastFarCallCost = gasBefore - gasleft();
    }
}

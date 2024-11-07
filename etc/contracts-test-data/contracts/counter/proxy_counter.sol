// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

interface ICounter {
    function increment(uint256 _increment) external;
    function incrementWithRevert(uint256 _increment, bool _shouldRevert) external returns (uint256);
    function get() external view returns (uint256);
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

    function testCounterCall(uint _expectedInitialValue) external {
        require(address(counter) != address(0), "counter not deployed");

        require(counter.get() == _expectedInitialValue, "counter.get()");
        counter.increment(3);
        require(counter.get() == _expectedInitialValue + 3, "counter.get() after");

        // Test catching reverts from EVM and EraVM contracts.
        try counter.incrementWithRevert(4, true) returns (uint) {
            revert("expected revert");
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("This method always reverts"), "unexpected error");
        }
        require(counter.get() == _expectedInitialValue + 3, "counter.get() after revert");
    }
}

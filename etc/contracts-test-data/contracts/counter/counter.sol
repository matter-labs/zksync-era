// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract Counter {
    uint256 value;

    function increment(uint256 x) public {
        value += x;
    }

    function incrementWithRevert(uint256 x, bool shouldRevert) public {
        value += x;
        if(shouldRevert) {
            revert("This method always reverts");
        }
    }

    function get() public view returns (uint256) {
        return value;
    }
}

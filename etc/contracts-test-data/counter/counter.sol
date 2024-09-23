// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract Counter {
    uint256 value;

    function increment(uint256 x) public {
        value += x;
    }

    function incrementWithRevertPayable(uint256 x, bool shouldRevert) public payable returns (uint256) {
        return incrementWithRevert(x, shouldRevert);
    }

    function incrementWithRevert(uint256 x, bool shouldRevert) public returns (uint256) {
        value += x;
        if (shouldRevert) {
            revert("This method always reverts");
        }
        return value;
    }

    function get() public view returns (uint256) {
        return value;
    }
}

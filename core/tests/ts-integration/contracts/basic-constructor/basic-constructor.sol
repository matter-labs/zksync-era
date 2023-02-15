// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

contract SimpleConstructor {
    uint256 c;

    constructor(uint256 a, uint256 b, bool shouldRevert) {
        c = a * b;
        require(!shouldRevert);
    }

    function get() public view returns (uint256) {
        return c;
    }
}

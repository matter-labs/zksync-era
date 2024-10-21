// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.0;

contract ConstructorRevert {
    uint256 value;

    constructor() {
        revert("Failure string");
    }
}

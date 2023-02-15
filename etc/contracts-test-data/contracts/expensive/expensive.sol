// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;
pragma abicoder v2;

contract Expensive {
    uint[] array;

    function expensive(uint iterations) public returns (bytes32) {
        for (uint i = 0; i < iterations; i++) {
            array.push(i);
        }
        return keccak256(abi.encodePacked(array));
    }
}

// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

contract Emitter {
    event Trivial();
    event Simple(uint256 Number, address Account);
    event Indexed(uint256 indexed Number, address Account);

    function test(uint256 number) public {
        emit Trivial();
        emit Simple(number, address(0xdeadbeef));
        emit Indexed(number, address(0xc0ffee));
    }

    function emitManyEvents(uint256 iterations) public {
        for (uint i = 0; i < iterations; i++) {
            emit Trivial();
        }
    }
}

// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;
pragma abicoder v2;

contract InfiniteLoop {
    event Iteration(uint256 number);

    function infiniteLoop() public {
        uint256 x = 0;

        while (true) {
            x += 1;
            // This event is needed so that LLVM
            // won't optimize the loop away.
            emit Iteration(x);
        }
    }
}

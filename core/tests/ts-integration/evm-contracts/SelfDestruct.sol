// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.0;

contract SelfDestruct {
    constructor() payable {}

    function destroy(address recipient) external {
        assembly {
            selfdestruct(recipient)
        }
    }
}

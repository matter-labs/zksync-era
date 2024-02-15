// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract StaticCallTester {
    function performStaticCall(address _target, bytes memory _data) external view {
        (bool success, bytes memory data) = _target.staticcall(_data);
        if (!success) {
            assembly {
                revert(add(data, 32), mload(data))
            }
        }
    }
}

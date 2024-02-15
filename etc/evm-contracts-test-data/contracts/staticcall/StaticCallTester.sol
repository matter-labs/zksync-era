// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract StaticCallTester {
    uint256 x;

    function readFromStorage() external view {
        require(x == 101, "Invalid value");
    }

    function writeToStorage() external {
        x = 102;
    }

    function callWriteStorage() external {
        this.writeToStorage();
    }

    function testInner() external {
        {
            (bool success, bytes memory data) = address(this).call(abi.encodeWithSignature("readFromStorage()"));
            require(success, "The static call should permit reads");
            require(data.length == 0, "The returndata should be empty");
        }

        {
            (bool success, bytes memory data) = address(this).call(abi.encodeWithSignature("writeToStorage()"));
            require(!success, "The static call should not permit writes");
            require(data.length == 0, "The returndata should be empty");
        }
        
        {
            (bool success, bytes memory data) = address(this).delegatecall(abi.encodeWithSignature("readFromStorage()"));
            require(success, "The static delegate call should permit reads");
            require(data.length == 0, "The returndata should be empty");
        }

        {
            (bool success, bytes memory data) = address(this).delegatecall(abi.encodeWithSignature("writeToStorage()"));
            require(!success, "The static delegate call should not permit writes");
            require(data.length == 0, "The returndata should be empty");
        }
    }

    function test() external {
        {
            (bool success,) = address(this).call(abi.encodeWithSignature("writeToStorage()"));
            require(success, "The normal call should permit writes");
        }

        x = 101;

        (bool success, bytes memory data) = address(this).staticcall(abi.encodeWithSignature("testInner()"));
        if (!success) {
            assembly {
                revert(add(data, 32), mload(data))
            }
        }
    }
}

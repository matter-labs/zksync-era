// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.7.0;

contract Creation {
    function blockNumber() external view returns (uint256) {
        return block.number;
    }
}

contract Creator {
    function create() external {
        new Creation();
    }

    function getCreationRuntimeCode() external pure returns (bytes memory) {
        return type(Creation).runtimeCode;
    }
}

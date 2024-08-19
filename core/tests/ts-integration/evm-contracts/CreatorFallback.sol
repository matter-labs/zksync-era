// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.7.0;

contract Creation {
    function blockNumber() external view returns (uint256) {
        return block.number;
    }
}

contract CreatorFallback {
    function performCall() external {
        new Creation();
        type(Creation).runtimeCode;
    }
    fallback() external {
        this.performCall();
    }
}

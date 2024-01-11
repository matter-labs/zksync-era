// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract Precompiles {
    function doKeccak(uint256 iters) public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < iters; i += 1) {
            sum += uint(keccak256(abi.encode(i))) % 256;
        }
        return sum;
    }

    function doSha256(uint256 iters) public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < iters; i += 1) {
            sum += uint(sha256(abi.encode(i))) % 256;
        }
        return sum;
    }
}

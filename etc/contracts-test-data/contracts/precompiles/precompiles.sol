// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

address constant CODE_ORACLE_ADDR = 0x0000000000000000000000000000000000008012;

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

    uint256 lastCodeOracleCost;
    function callCodeOracle(bytes32 _versionedHash, bytes32 _expectedBytecodeHash) public {
        uint256 gasBefore = gasleft();

        // Call the code oracle
        (bool success, bytes memory returnedBytecode) = CODE_ORACLE_ADDR.staticcall(abi.encodePacked(_versionedHash));

        lastCodeOracleCost = gasBefore - gasleft();

        // Check the result
        require(success, "CodeOracle call failed");

        // Check the returned bytecode
        require(keccak256(returnedBytecode) == _expectedBytecodeHash, "Returned bytecode does not match the expected hash");
    }
}

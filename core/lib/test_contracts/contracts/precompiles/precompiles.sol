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

    /// Calls `CodeOracle` `_count` times with a *zero-length* output buffer, so the caller never
    /// copies the returned bytecode. Used to check what the call tracer retains per call.
    function callCodeOracleMany(bytes32 _versionedHash, uint256 _count) public returns (uint256) {
        address oracle = CODE_ORACLE_ADDR;
        uint256 gasBefore = gasleft();
        assembly {
            mstore(0, _versionedHash)
            for { let i := 0 } lt(i, _count) { i := add(i, 1) } {
                // Zero output offset and size: the caller requests no returndata at all.
                if iszero(staticcall(gas(), oracle, 0, 32, 0, 0)) {
                    revert(0, 0)
                }
            }
        }
        uint256 cost = gasBefore - gasleft();
        lastCodeOracleCost = cost;
        return cost;
    }

    /// Reverts with `_size` bytes of data, to check what a call trace retains for revert reasons.
    function revertWithLargeData(uint256 _size) public pure {
        assembly {
            revert(0, _size)
        }
    }

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

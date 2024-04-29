// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract BenchmarkCaller {
    uint256 _resultGas;

    function callAndBenchmark(address _to) external returns (uint256){
        uint256 startGas = gasleft();
        // Just doing a call to an address
        (bool success, ) = _to.call("");
        require(success);
        _resultGas = startGas - gasleft();
        return _resultGas;
    }
}

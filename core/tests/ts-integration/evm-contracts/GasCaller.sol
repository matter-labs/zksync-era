// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract GasCaller {
    uint256 _resultGas;

    function callAndGetGas(address _to) external returns (uint256){
        uint256 startGas = gasleft();
        // Just doing a call to an address
        (bool success, ) = _to.call("");
        require(success);
        _resultGas = startGas - gasleft();
        return _resultGas;
    }
}

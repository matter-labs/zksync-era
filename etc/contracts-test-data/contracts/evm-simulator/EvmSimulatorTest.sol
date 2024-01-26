// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

import { SystemContractHelper } from "./SystemContractHelper.sol"; 

/// Test for various VM invariants
contract EvmSimulatorTest {
    uint256 constant EXPECTED_STIPEND = (1 << 30);

    uint256 storageVariable;

    function getGas() external {
        storageVariable = gasleft();
    }    

    function getEVMGas(uint256 _ratio) external view returns (uint256 res) {
        uint256 gasLeft = gasleft();

        unchecked {
        
        if(gasLeft > EXPECTED_STIPEND) {
            res = (gasLeft - EXPECTED_STIPEND) / _ratio;
        } else {
            res = 0;
        }

        }
    }

    function testGas(uint256 _gasToPass, uint256 _ratio) external {
        uint256 obtainedEVMGas = this.getEVMGas{gas: _gasToPass}(_ratio);

        storageVariable = obtainedEVMGas;
    } 

    function testStaticCallInner() external {
        // 1. Ensuring that we are in static call
        uint256 callFlags = SystemContractHelper.getCallFlags();
        // TODO: make it a constnat
        uint256 isEVMInStaticContext = callFlags & 0x04;

        require(isEVMInStaticContext != 0, "Not in static call");

        // EVM can ignore "native" static call
        storageVariable = 1;
    }

    function testStaticCall() external view {
        // The function will be called directly without the use of `staticcall`.

        bytes4 selector = bytes4(keccak256("testStaticCallInner()"));

        bool result;
        assembly {
            mstore(0, selector)
            result := staticcall(gas(), address(), 0, 4, 0, 0)
        }

        require(result, "Static call failed");
    }
}

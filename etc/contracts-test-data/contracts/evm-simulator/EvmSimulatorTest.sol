// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

import { SystemContractHelper } from "./SystemContractHelper.sol"; 
import { DEPLOYER_SYSTEM_CONTRACT } from "./Constants.sol";

/// Test for various VM invariants
contract EvmSimulatorTest {
    constructor() {
        // We are just returning the same bytecode
        bytes memory bytecodeToExecute = getMyConstructorCode();
        DEPLOYER_SYSTEM_CONTRACT.setDeployedCode(
            100,
            bytecodeToExecute
        );
    }

    function getMyConstructorCode() internal view returns (bytes memory bytecode) {
        SystemContractHelper.loadCalldataIntoActivePtr();
        uint256 size = SystemContractHelper.getActivePtrDataSize();

        bytecode = new bytes(size);

        uint256 dest;
        assembly {
            dest := add(bytecode, 32)
        }

        SystemContractHelper.copyActivePtrData(dest, 0, size);
    }

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

    function getReturndata(bytes memory _calldata) external pure returns (bytes memory returndata) {
        returndata = _calldata;
    }

    function testRememberingReturndata() external view {
        this.getReturndata(hex"babacaca");
        // Now we should be able to preserve the returndata
        SystemContractHelper.loadReturndataIntoActivePtr();

        this.getReturndata(hex"00");
        // Firstly, let's check that the length was restored correctly
        uint256 returndataLength = SystemContractHelper.getActivePtrDataSize();

        // It is 96 because it is a properly encoded bytes array;
        // offset (32 bytes) + length (32 bytes) + padded hex"babacaca" (32 bytes)
        require(returndataLength == 96, "Returndata length was not restored correctly");

        bytes memory returndata;
        {
            bytes memory previousReturnData = new bytes(96);
            uint256 location;
            assembly {
                location := add(previousReturnData, 32)
            }
            SystemContractHelper.copyActivePtrData(location, 0, 96);

            returndata = abi.decode(previousReturnData, (bytes));
        }

        require(keccak256(returndata) == keccak256(hex"babacaca"), "Correct returndata caching");
    }
}

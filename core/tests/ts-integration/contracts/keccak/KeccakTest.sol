// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;
pragma abicoder v2;

import "../custom-account/SystemContractsCaller.sol";
import "../custom-account/Constants.sol";

contract KeccakTest {
    // Just some computation-heavy function, it will be used to test out of gas
    function infiniteFuction(uint256 n) pure public returns (uint256 sumOfSquares) {
        for(uint i = 0; i < n; i++) {
            sumOfSquares += i * i;
        }
    }

    function _loadFarCallABIIntoActivePtr(
        uint256 _gas
    ) private view {
        uint256 farCallAbi = SystemContractsCaller.getFarCallABIWithEmptyFatPointer(
            uint32(_gas),
            // Only rollup is supported for now
            0,
            CalldataForwardingMode.ForwardFatPointer,
            false,
            false
        );
        _ptrPackIntoActivePtr(farCallAbi);
    }

    function _loadReturnDataIntoActivePtr() internal {
        address callAddr = LOAD_LATEST_RETURNDATA_INTO_ACTIVE_PTR_CALL_ADDRESS;
        assembly {
            pop(staticcall(0, callAddr, 0, 0xFFFF, 0, 0))
        }
    }

    function _ptrPackIntoActivePtr(uint256 _farCallAbi) internal view {
        address callAddr = PTR_PACK_INTO_ACTIVE_CALL_ADDRESS;
        assembly {
            pop(staticcall(_farCallAbi, callAddr, 0, 0xFFFF, 0, 0))
        }
    }

    function rawCallByRef(address _address) internal returns (bool success) {
        address callAddr = RAW_FAR_CALL_BY_REF_CALL_ADDRESS;
        assembly {
            success := call(_address, callAddr, 0, 0, 0xFFFF, 0, 0)
        }
    }

    function zeroPointerTest() external {
        try this.infiniteFuction{gas: 1000000}(1000000) returns (uint256 sumOfSquares) {
            revert("The transaction should have failed");
        } catch {}

        _loadReturnDataIntoActivePtr();
        _loadFarCallABIIntoActivePtr(1000000);
        bool success = rawCallByRef(KECCAK256_SYSTEM_CONTRACT);
        require(success, "The call to keccak should have succeeded");

        uint256 returndataSize = 0;
        assembly {
            returndataSize := returndatasize()
        }
        require(returndataSize == 32, "The return data size should be 32 bytes");

        bytes32 result;
        assembly {
            let ptr := mload(0x40)
            returndatacopy(ptr, 0, 32)
            result := mload(ptr)
            // Clearing the pointer just in case
            mstore(ptr, 0)
        }

        require(result == bytes32(0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470), "The result is not correct");
    }
}

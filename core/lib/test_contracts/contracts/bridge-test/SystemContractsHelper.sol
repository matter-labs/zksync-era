// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

import {SystemContractsCaller, CalldataForwardingMode} from "l1-contracts/common/l2-helpers/SystemContractsCaller.sol";

address constant MIMIC_CALL_CALL_ADDRESS = address((1 << 16) - 5);

library SystemContractsHelper {
    /// @notice Performs a `mimicCall` to an address.
    /// @param _to The address to call.
    /// @param _whoToMimic The address to mimic.
    /// @param _data The data to pass to the call.
    /// @return success Whether the call was successful.
    /// @return returndata The return data of the call.
    function mimicCall(
        address _to,
        address _whoToMimic,
        bytes memory _data
    ) internal returns (bool success, bytes memory returndata) {
        // In zkSync, no memory-related values can exceed uint32, so it is safe to convert here
        uint32 dataStart;
        uint32 dataLength = uint32(_data.length);
        assembly {
            dataStart := add(_data, 0x20)
        }

        uint256 farCallAbi = SystemContractsCaller.getFarCallABI({
            dataOffset: 0,
            memoryPage: 0,
            dataStart: dataStart,
            dataLength: dataLength,
            gasPassed: uint32(gasleft()),
            shardId: 0,
            forwardingMode: CalldataForwardingMode.UseHeap,
            isConstructorCall: false,
            isSystemCall: false
        });

        address callAddr = MIMIC_CALL_CALL_ADDRESS;
        uint256 rtSize;
        assembly {
            success := call(_to, callAddr, 0, farCallAbi, _whoToMimic, 0, 0)
            rtSize := returndatasize()
        }

        returndata = new bytes(rtSize);
        assembly {
            returndatacopy(add(returndata, 0x20), 0, rtSize)
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

import {MIMIC_CALL_CALL_ADDRESS, SystemContractsCaller, CalldataForwardingMode} from "../custom-account/SystemContractsCaller.sol";
import "../custom-account/interfaces/IContractDeployer.sol";

import { DEPLOYER_SYSTEM_CONTRACT, FORCE_DEPLOYER } from "../custom-account/Constants.sol";
import "./msg-sender.sol";

contract ComplexUpgrade {
    constructor() {}

    struct MimicCallInfo {
        address to;
        address whoToMimic;
        bytes data;
    }

    function _mimicCall(MimicCallInfo memory info) internal {
        address callAddr = MIMIC_CALL_CALL_ADDRESS;

        bytes memory data = info.data;
        address to = info.to;
        address whoToMimic = info.whoToMimic;

        uint32 dataStart;
        uint32 dataLength;
        assembly {
            dataStart := add(data, 0x20)
            dataLength := mload(data)
        }

        uint256 farCallAbi = SystemContractsCaller.getFarCallABI(
            0,
            0,
            dataStart,
            dataLength,
            uint32(gasleft()),
            // Only rollup is supported for now
            0,
            CalldataForwardingMode.UseHeap,
            false,
            true
        );

        assembly {
            let success := call(to, callAddr, 0, farCallAbi, whoToMimic, 0, 0)

            if iszero(success) {
                returndatacopy(0, 0, returndatasize())
                revert(0, returndatasize())
            }
        }
    }

    function mimicCalls(
        MimicCallInfo[] memory info
    ) public {
        for (uint256 i = 0; i < info.length; i++) {
            _mimicCall(info[i]);
        }
    }

    // This function is used to imitate some complex upgrade logic
    function someComplexUpgrade(
        address _address1,
        address _address2,
        bytes32 _bytecodeHash
    ) external {
        IContractDeployer.ForceDeployment memory forceDeployment1 = IContractDeployer.ForceDeployment(
            _bytecodeHash,
            _address1,
            false,
            0,
            new bytes(0)
        );
        
        IContractDeployer.ForceDeployment memory forceDeployment2 = IContractDeployer.ForceDeployment(
            _bytecodeHash,
            _address2,
            false,
            0,
            new bytes(0)
        );

        IContractDeployer.ForceDeployment[] memory deploymentInput1 = new IContractDeployer.ForceDeployment[](1);
        deploymentInput1[0] = forceDeployment1;

        IContractDeployer.ForceDeployment[] memory deploymentInput2 = new IContractDeployer.ForceDeployment[](1);
        deploymentInput2[0] = forceDeployment2;

        DEPLOYER_SYSTEM_CONTRACT.forceDeployOnAddresses(deploymentInput1);
        DEPLOYER_SYSTEM_CONTRACT.forceDeployOnAddresses(deploymentInput2);

        // Here we also test the fact that complex upgrade implementation can use mimicCall
        MsgSenderTest msgSenderTest = new MsgSenderTest();
        address toMimic = address(0x1);
        bytes memory _mimicCallCalldata = abi.encodeWithSelector(
            MsgSenderTest.testMsgSender.selector,
            toMimic
        );

        MimicCallInfo memory info = MimicCallInfo({
            to: address(msgSenderTest),
            whoToMimic: toMimic,
            data: _mimicCallCalldata
        });

        _mimicCall(info);
    }
}

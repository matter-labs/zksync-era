// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;
import {EVM_GAS_MANAGER, INF_PASS_GAS} from "./Constants.sol";


contract GasManagerCaller {
    function warmAccount(address _addr) internal returns (bool isWarm) {
        bytes4 selector = EVM_GAS_MANAGER.warmAccount.selector;
        address addr = address(EVM_GAS_MANAGER);
        assembly {
            mstore(0, selector)
            mstore(4, _addr)

            let success := call(gas(), addr, 0, 0, 36, 0, 32)

            if iszero(success) {
                // This error should never happen
                revert(0, 0)
            }

            isWarm := mload(0)
        }
    }
    function warmSlot(uint256 key, uint256 currentValue) internal returns (bool isWarm, uint256 originalValue) {
        bytes4 selector = EVM_GAS_MANAGER.warmSlot.selector;
        address addr = address(EVM_GAS_MANAGER);
        assembly {
            mstore(0, selector)
            mstore(4, key)
            mstore(36, currentValue)

            let success := call(gas(), addr, 0, 0, 68, 0, 64)

            if iszero(success) {
                // This error should never happen
                revert(0, 0)
            }

            isWarm := mload(0)
            originalValue := mload(32)
        }
    }
     function _pushEVMFrame(uint256 _passGas, bool _isStatic) internal {
        bytes4 selector = EVM_GAS_MANAGER.pushEVMFrame.selector;
        address addr = address(EVM_GAS_MANAGER);
        assembly {
            mstore(0, selector)
            mstore(4, _passGas)
            mstore(36, _isStatic)

            let success := call(gas(), addr, 0, 0, 68, 0, 0)

            if iszero(success) {
                // This error should never happen
                revert(0, 0)
            }
        }
    }
    function _popEVMFrame() internal {
        bytes4 selector = EVM_GAS_MANAGER.popEVMFrame.selector;
        address addr = address(EVM_GAS_MANAGER);
        assembly {
            mstore(0, selector)

            let success := call(gas(), addr, 0, 0, 4, 0, 0)

            if iszero(success) {
                // This error should never happen
                revert(0, 0)
            }
        }
    }
    function _consumeEvmFrame() internal returns (uint256 _passGas, bool isStatic, bool callerEVM) {
        bytes4 selector = EVM_GAS_MANAGER.consumeEvmFrame.selector;
        address addr = address(EVM_GAS_MANAGER);
        assembly {
            mstore(0, selector)

            let success := call(gas(), addr, 0, 0, 4, 0, 64)

            if iszero(success) {
                // This error should never happen
                revert(0, 0)
            }

            _passGas := mload(0)
            isStatic := mload(32)
        }

        if (_passGas != INF_PASS_GAS) {
            callerEVM = true;
        }
    }
}

// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;
import {EVM_GAS_MANAGER, INF_PASS_GAS} from "./Constants.sol";


contract GasManagerCaller {
    function warmAccount(address _addr) public {
        EVM_GAS_MANAGER.warmAccount(_addr);
    }
    function warmSlot(uint256 key, uint256 currentValue) public {
        EVM_GAS_MANAGER.warmSlot(key, currentValue);
    }
     function _pushEVMFrame(uint256 _passGas, bool _isStatic) public {
        EVM_GAS_MANAGER.pushEVMFrame(_passGas, _isStatic);
    }
    function _popEVMFrame() public {
        EVM_GAS_MANAGER.popEVMFrame();
    }
    function _consumeEvmFrame() public {
        EVM_GAS_MANAGER.consumeEvmFrame();
    }
}

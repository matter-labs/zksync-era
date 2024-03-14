// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

/**
 * @author Matter Labs
 * @dev The interface that is used for encoding/decoding of
 * different types of paymaster flows.
 * @notice This is NOT an interface to be implementated
 * by contracts. It is just used for encoding.
 */

interface IEvmGasManager {
   function warmAccount(address account) external payable returns (bool wasWarm);

   function warmSlot(uint256 _slot, uint256 _currentValue) external payable returns (bool, uint256);

   function pushEVMFrame(uint256 _passGas, bool _isStatic) external;

   function consumeEvmFrame() external returns (uint256 passGas, bool isStatic);

   function popEVMFrame() external;
}

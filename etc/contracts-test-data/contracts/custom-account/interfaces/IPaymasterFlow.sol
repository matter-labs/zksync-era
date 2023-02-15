// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

/**
 * @author Matter Labs
 * @dev The interface that is used for encoding/decoding of
 * different types of paymaster flows.
 * @notice This is NOT an interface to be implementated
 * by contracts. It is just used for encoding.
 */
interface IPaymasterFlow {
    function general(bytes calldata input) external;

    function approvalBased(address _token, uint256 _minAllowance, bytes calldata _innerInput) external;
}

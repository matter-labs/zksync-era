// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {IPaymaster, ExecutionResult, PAYMASTER_VALIDATION_SUCCESS_MAGIC} from "../custom-account/interfaces/IPaymaster.sol";
import {IPaymasterFlow} from "../custom-account/interfaces/IPaymasterFlow.sol";
import {TransactionHelper, Transaction} from "../custom-account/TransactionHelper.sol";

import "../custom-account/Constants.sol";

contract Paymaster is IPaymaster {
    uint256 constant PRICE_FOR_PAYING_FEES = 1;

    modifier onlyBootloader() {
        require(
            msg.sender == BOOTLOADER_FORMAL_ADDRESS,
            "Only bootloader can call this method"
        );
        // Continue execution if called from the bootloader.
        _;
    }

    constructor() {}

    function validateAndPayForPaymasterTransaction(
        bytes32,
        bytes32,
        Transaction calldata _transaction
    ) external payable returns (bytes4 magic, bytes memory context) {
        // By default we consider the transaction as accepted.
        magic = PAYMASTER_VALIDATION_SUCCESS_MAGIC;
        require(
            _transaction.paymasterInput.length >= 4,
            "The standard paymaster input must be at least 4 bytes long"
        );
        
        uint256 txNonce = _transaction.nonce;

        if (txNonce == 0) {
            revert("Nonce is zerooo");
        }

        // Note, that while the minimal amount of ETH needed is tx.gasPrice * tx.gasLimit,
        // neither paymaster nor account are allowed to access this context variable.
        uint256 requiredETH = _transaction.gasLimit * _transaction.maxFeePerGas;

        // The bootloader never returns any data, so it can safely be ignored here.
        (bool success, ) = payable(BOOTLOADER_FORMAL_ADDRESS).call{
            value: requiredETH
        }("");
        require(success, "Failed to transfer funds to the bootloader");
    }

    function postTransaction(
        bytes calldata _context,
        Transaction calldata _transaction,
        bytes32,
        bytes32,
        ExecutionResult _txResult,
        uint256 _maxRefundedGas
    ) external payable override {
        // Refunds are not supported yet.
    }

    receive() external payable {}
}

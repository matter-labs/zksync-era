// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

import "../TransactionHelper.sol";

bytes4 constant ACCOUNT_VALIDATION_SUCCESS_MAGIC = IAccount.validateTransaction.selector;

interface IAccount {
    /// @notice Called by the bootloader to validate that an account agrees to process the transaction
    /// (and potentially pay for it). 
    /// @param _txHash The hash of the transaction to be used in the explorer
    /// @param _suggestedSignedHash The hash of the transaction is signed by EOAs
    /// @param _transaction The transaction itself
    /// @return magic The magic value that should be equal to the signature of this function 
    /// if the user agrees to proceed with the transaction.
    /// @dev The developer should strive to preserve as many steps as possible both for valid
    /// and invalid transactions as this very method is also used during the gas fee estimation 
    /// (without some of the necessary data, e.g. signature).
    function validateTransaction(
        bytes32 _txHash,
        bytes32 _suggestedSignedHash,
        Transaction calldata _transaction
    ) external payable returns (bytes4 magic);

    function executeTransaction(
        bytes32 _txHash,
        bytes32 _suggestedSignedHash,
        Transaction calldata _transaction
    ) external payable;

    // There is no point in providing possible signed hash in the `executeTransactionFromOutside` method,
    // since it typically should not be trusted.
    function executeTransactionFromOutside(Transaction calldata _transaction) external payable;

    function payForTransaction(
        bytes32 _txHash,
        bytes32 _suggestedSignedHash,
        Transaction calldata _transaction
    ) external payable;

    function prepareForPaymaster(
        bytes32 _txHash,
        bytes32 _possibleSignedHash,
        Transaction calldata _transaction
    ) external payable;
}

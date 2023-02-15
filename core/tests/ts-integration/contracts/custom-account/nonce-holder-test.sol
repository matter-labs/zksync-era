// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

import './Constants.sol';
import './TransactionHelper.sol';

import './interfaces/IAccount.sol';
import './interfaces/IContractDeployer.sol';

import './SystemContractsCaller.sol';

/**
* @author Matter Labs
* @dev Dummy account used for tests that accepts any transaction.
*/
contract NonceHolderTest is IAccount {
    using TransactionHelper for Transaction;

    // bytes4(keccak256("isValidSignature(bytes32,bytes)")
    bytes4 constant EIP1271_SUCCESS_RETURN_VALUE = 0x1626ba7e;

    function validateTransaction(bytes32, bytes32, Transaction calldata _transaction) external payable override returns (bytes4 magic) {
        // By default we consider the transaction as successful
        magic = ACCOUNT_VALIDATION_SUCCESS_MAGIC;
                
        _validateTransaction(_transaction);
    }

    function _validateTransaction(Transaction calldata _transaction) internal {
        bytes memory data;

        if (uint8(_transaction.signature[0]) == 0) {
            // It only erases nonce as non-allowed			
            data = abi.encodeCall(NONCE_HOLDER_SYSTEM_CONTRACT.setValueUnderNonce, (_transaction.nonce, 1));
        } else if(uint8(_transaction.signature[0]) == 1) {
            // It should increase minimal nonce by 5
            data = abi.encodeCall(NONCE_HOLDER_SYSTEM_CONTRACT.increaseMinNonce, (5));
        } else if(uint8(_transaction.signature[0]) == 2) {
            // It should try increasing nnonce by 2**90
            data = abi.encodeCall(NONCE_HOLDER_SYSTEM_CONTRACT.increaseMinNonce, (2**90));
        } else if (uint8(_transaction.signature[0]) == 3) {
            // Do nothing
            return;
        } else if(uint8(_transaction.signature[0]) == 4) {
            // It should increase minimal nonce by 1
            data = abi.encodeCall(NONCE_HOLDER_SYSTEM_CONTRACT.increaseMinNonce, (1));
        } else if (uint8(_transaction.signature[0]) == 5) {
            // Increase minimal nonce by 5 and set the nonce ordering of the account as arbitrary
            data = abi.encodeCall(NONCE_HOLDER_SYSTEM_CONTRACT.increaseMinNonce, (5));
            SystemContractsCaller.systemCallWithPropagatedRevert(
                uint32(gasleft()),
                address(DEPLOYER_SYSTEM_CONTRACT),
                0,
                abi.encodeCall(DEPLOYER_SYSTEM_CONTRACT.updateNonceOrdering, (IContractDeployer.AccountNonceOrdering.Arbitrary))
            );
        } else {
            revert("Unsupported test");
        }

        SystemContractsCaller.systemCallWithPropagatedRevert(
            uint32(gasleft()),
            address(NONCE_HOLDER_SYSTEM_CONTRACT),
            0,
            data
        );
    }

    function executeTransaction(bytes32, bytes32, Transaction calldata _transaction) external payable override {
        _execute(_transaction);
    }

    function executeTransactionFromOutside(Transaction calldata _transaction) external payable override {
        _validateTransaction(_transaction);
        _execute(_transaction);
    }

    function _execute(Transaction calldata _transaction) internal {}

    // Here, the user pays the bootloader for the transaction
    function payForTransaction(bytes32, bytes32, Transaction calldata _transaction) external payable override {
        bool success = _transaction.payToTheBootloader();
        require(success, "Failed to pay the fee to the operator");
    }

    // Here, the user should prepare for the transaction to be paid for by a paymaster
    // Here, the account should set the allowance for the smart contracts
    function prepareForPaymaster(bytes32, bytes32, Transaction calldata _transaction) external payable override {
        _transaction.processPaymasterInput();
    }

    fallback() external payable {
        // fallback of default AA shouldn't be called by bootloader under no circumstances 
        assert(msg.sender != BOOTLOADER_FORMAL_ADDRESS);		
        
        // If the contract is called directly, behave like an EOA
    }

    receive() external payable {}
}

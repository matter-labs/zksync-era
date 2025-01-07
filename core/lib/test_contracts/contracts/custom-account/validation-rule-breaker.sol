// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

import "./Constants.sol";
import "./TransactionHelper.sol";

import "./SystemContractsCaller.sol";

import "./interfaces/IAccount.sol";

contract ValidationRuleBreaker is IAccount {
    using TransactionHelper for Transaction;

    uint32 public typeOfRuleBreak;
    address public trustedAddress = address(0x800a);

    constructor() {
        typeOfRuleBreak = 0;
    }

    function setTypeOfRuleBreak(uint32 _typeOfRuleBreak) external {
        typeOfRuleBreak = _typeOfRuleBreak;
    }

    function validateTransaction(
        bytes32 _txHash,
        bytes32 _suggestedSignedTxHash,
        Transaction calldata _transaction
    ) external payable override returns (bytes4 magic) {
        // By default we consider the transaction as successful
        magic = VALIDATION_SUCCESS_MAGIC;

        if (typeOfRuleBreak == 1) {
            // The balance of another account may not be read
            // I'm writing assertions because otherwise the compiler would optimize away the access
            require(BOOTLOADER_FORMAL_ADDRESS.balance != 0);
        } else if (typeOfRuleBreak == 2) {
            // May not call an EOA
            address(1234567890).call("");
        } else if (typeOfRuleBreak == 3) {
            // This should succeed because a trustedAddress is marked as a slot that grants access to the address it contains
            require(trustedAddress == address(0x800a));
            require(BOOTLOADER_FORMAL_ADDRESS.balance != 0);
        } else if (typeOfRuleBreak == 4) {
            // This should still fail; EIP-4337 defines out of gas as an immediate failure
            address(this).call(
                abi.encodeWithSignature("_runOutOfGasButCatchThePanic()")
            );
        }

        _validateTransaction(_suggestedSignedTxHash, _transaction);
    }

    function _runOutOfGasButCatchThePanic() external {
        address(this).call(
            abi.encodeWithSignature("_runOutOfGasButCatchThePanic()")
        );
    }

    function _validateTransaction(
        bytes32 _suggestedSignedTxHash,
        Transaction calldata _transaction
    ) internal {
        SystemContractsCaller.systemCallWithPropagatedRevert(
            uint32(gasleft()),
            address(NONCE_HOLDER_SYSTEM_CONTRACT),
            0,
            abi.encodeCall(
                INonceHolder.incrementMinNonceIfEquals,
                (_transaction.nonce)
            )
        );
    }

    function executeTransaction(
        bytes32,
        bytes32,
        Transaction calldata _transaction
    ) external payable override {
        _execute(_transaction);
    }

    function executeTransactionFromOutside(
        Transaction calldata _transaction
    ) external payable override {
        _validateTransaction(bytes32(0), _transaction);
        _execute(_transaction);
    }

    function _execute(Transaction calldata _transaction) internal {
        address to = address(uint160(_transaction.to));
        uint256 value = _transaction.reserved[1];
        bytes memory data = _transaction.data;

        if (to == address(DEPLOYER_SYSTEM_CONTRACT)) {
            // We allow calling ContractDeployer with any calldata
            SystemContractsCaller.systemCallWithPropagatedRevert(
                uint32(gasleft()),
                to,
                uint128(_transaction.reserved[1]), // By convention, reserved[1] is `value`
                _transaction.data
            );
        } else {
            bool success;
            assembly {
                success := call(
                    gas(),
                    to,
                    value,
                    add(data, 0x20),
                    mload(data),
                    0,
                    0
                )
            }
            require(success);
        }
    }

    // Here, the user pays the bootloader for the transaction
    function payForTransaction(
        bytes32,
        bytes32,
        Transaction calldata _transaction
    ) external payable {
        bool success = _transaction.payToTheBootloader();
        require(success, "Failed to pay the fee to the operator");
    }

    // Here, the user should prepare for the transaction to be paid for by a paymaster
    // Here, the account should set the allowance for the smart contracts
    function prepareForPaymaster(
        bytes32,
        bytes32,
        Transaction calldata _transaction
    ) external payable {
        _transaction.processPaymasterInput();
    }

    fallback() external payable {
        // fallback of default AA shouldn't be called by bootloader under no circumstances
        assert(msg.sender != BOOTLOADER_FORMAL_ADDRESS);

        // If the contract is called directly, behave like an EOA
    }

    receive() external payable {}
}

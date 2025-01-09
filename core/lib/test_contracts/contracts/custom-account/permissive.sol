// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

import './interfaces/IAccount.sol';
import './interfaces/INonceHolder.sol';
import './SystemContractsCaller.sol';
import './TransactionHelper.sol';

/** Account abstraction that can be called as a contract. */
contract PermissiveAccount is IAccount {
    using TransactionHelper for Transaction;

    function deploy(uint _count) external {
        for (uint i = 0; i < _count; i++) {
            new PermissiveAccountDeployedContract();
        }
    }

    function validateTransaction(
        bytes32,
        bytes32,
        Transaction calldata _transaction
    ) external payable override returns (bytes4 magic) {
        SystemContractsCaller.systemCallWithPropagatedRevert(
            uint32(gasleft()),
            address(0x8003), // NonceHolder address
            0,
            abi.encodeCall(
                INonceHolder.incrementMinNonceIfEquals,
                (_transaction.nonce)
            )
        );

        magic = VALIDATION_SUCCESS_MAGIC; // always succeed
    }

    function executeTransaction(
        bytes32,
        bytes32,
        Transaction calldata _transaction
    ) external payable override {
        _execute(_transaction);
    }

    function executeTransactionFromOutside(Transaction calldata _transaction) external payable override {
        _execute(_transaction);
    }

    function _execute(Transaction calldata _transaction) internal {
        address to = address(uint160(_transaction.to));
        uint256 value = _transaction.value;
        bytes memory data = _transaction.data;

        if (to == address(0x8006)) { // ContractDeployer
            // `ContractDeployer` deployments require a system call
            SystemContractsCaller.systemCallWithPropagatedRevert(
                uint32(gasleft()),
                to,
                uint128(value),
                _transaction.data
            );
        } else {
            (bool success, bytes memory returnData) = to.call{value: value}(data);
            if (!success) {
                assembly {
                    let size := mload(returnData)
                    revert(add(returnData, 0x20), size)
                }
            }
        }
    }

    function payForTransaction(
        bytes32,
        bytes32,
        Transaction calldata _transaction
    ) external payable override {
        bool success = _transaction.payToTheBootloader();
        require(success, "Failed to pay the fee to the operator");
    }

    function prepareForPaymaster(
        bytes32,
        bytes32,
        Transaction calldata _transaction
    ) external payable override {
        _transaction.processPaymasterInput();
    }

    // Behave like EOA by default
    fallback() external payable {}
    receive() external payable {}
}

contract PermissiveAccountDeployedContract {}

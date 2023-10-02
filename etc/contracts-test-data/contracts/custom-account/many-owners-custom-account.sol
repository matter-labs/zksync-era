// SPDX-License-Identifier: MIT OR Apache-2.0

// Example custom account contract, that allows any of the selected owners to act on its behalf.
// This contract is for UNITTESTS ONLY. Do NOT use in production.
pragma solidity ^0.8.0;

import "./Constants.sol";
import "./TransactionHelper.sol";

import "./SystemContractsCaller.sol";

import "./interfaces/IAccount.sol";

contract ManyOwnersCustomAccount is IAccount {
    using TransactionHelper for Transaction;

    address[] public owners;

    // Sets the current owners.
    // This is NOT safe, this contract is for unittests ONLY.
    function setOwners(address[] memory _owners) external {
        owners = _owners;
    }

    // bytes4(keccak256("isValidSignature(bytes32,bytes)")
    bytes4 constant EIP1271_SUCCESS_RETURN_VALUE = 0x1626ba7e;

    function validateTransaction(
        bytes32,
        bytes32 _suggestedSignedTxHash,
        Transaction calldata _transaction
    ) external payable override returns (bytes4 magic) {
        // By default we consider the transaction as successful
        magic = VALIDATION_SUCCESS_MAGIC;

        _validateTransaction(_suggestedSignedTxHash, _transaction);
    }

    function _validateTransaction(
        bytes32 _suggestedSignedTxHash,
        Transaction calldata _transaction
    ) internal {
        if (_suggestedSignedTxHash == bytes32(0)) {
            _suggestedSignedTxHash = _transaction.encodeHash();
        }

        SystemContractsCaller.systemCallWithPropagatedRevert(
            uint32(gasleft()),
            address(NONCE_HOLDER_SYSTEM_CONTRACT),
            0,
            abi.encodeCall(
                INonceHolder.incrementMinNonceIfEquals,
                (_transaction.nonce)
            )
        );

        bytes memory _signature = _transaction.signature;

        uint8 v;
        bytes32 r;
        bytes32 s;
        // Signature loading code
        // we jump 32 (0x20) as the first slot of bytes contains the length
        // we jump 65 (0x41) per signature
        // for v we load 32 bytes ending with v (the first 31 come from s) then apply a mask
        assembly {
            r := mload(add(_signature, 0x20))
            s := mload(add(_signature, 0x40))
            v := and(mload(add(_signature, 0x41)), 0xff)
        }
        require(v == 27 || v == 28, "v is neither 27 nor 28");
        require(
            uint256(s) <=
                0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5D576E7357A4501DDFE92F46681B20A0,
            "Invalid s"
        );

        address recoveredAddress = ecrecover(_suggestedSignedTxHash, v, r, s);

        for (uint i = 0; i < owners.length; i++) {
            if (recoveredAddress == owners[i]) {
                return;
            }
        }
        revert("Invalid signature");
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
        uint256 value = _transaction.value;

        bytes memory data = _transaction.data;
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

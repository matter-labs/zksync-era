// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

import './Constants.sol';
import './TransactionHelper.sol';

import './SystemContractsCaller.sol';

import './interfaces/IAccount.sol';

contract CustomAccount is IAccount {
	using TransactionHelper for Transaction;

	bool public violateValidationRules;

	bytes32 public lastTxHash;

	constructor(bool _violateValidationRules) {
		violateValidationRules = _violateValidationRules;
	}

	// bytes4(keccak256("isValidSignature(bytes32,bytes)")
	bytes4 constant EIP1271_SUCCESS_RETURN_VALUE = 0x1626ba7e;

	function validateTransaction(bytes32 _txHash, bytes32 _suggestedSignedTxHash, Transaction calldata _transaction) external payable override returns (bytes4 magic) {
		// By default we consider the transaction as successful
        magic = VALIDATION_SUCCESS_MAGIC;

		_validateTransaction(_suggestedSignedTxHash, _transaction);
		lastTxHash = _txHash;

		if (violateValidationRules) {
			// Such a tx should not pass the validation step, because it depends on the balance of another account
			require(BOOTLOADER_FORMAL_ADDRESS.balance == 0, "Bootloader balance must be zero");
		}
	}

	function _validateTransaction(bytes32 _suggestedSignedTxHash, Transaction calldata _transaction) internal {
		if (_suggestedSignedTxHash == bytes32(0)) {
			_suggestedSignedTxHash = _transaction.encodeHash();
		}

		SystemContractsCaller.systemCallWithPropagatedRevert(
			uint32(gasleft()),
			address(NONCE_HOLDER_SYSTEM_CONTRACT),
			0,
			abi.encodeCall(INonceHolder.incrementMinNonceIfEquals, (_transaction.nonce))
		);
				
		bytes memory correctSignature = abi.encodePacked(_suggestedSignedTxHash, address(this));
		require(keccak256(_transaction.signature) == keccak256(correctSignature), "Incorrect signature");
	}

	function executeTransaction(bytes32, bytes32, Transaction calldata _transaction) external payable override {
		_execute(_transaction);
	}

	function executeTransactionFromOutside(Transaction calldata _transaction) external payable override {
		_validateTransaction(bytes32(0), _transaction);
		_execute(_transaction);
	}

	function _execute(Transaction calldata _transaction) internal {
		address to = address(uint160(_transaction.to));
		uint256 value = _transaction.reserved[1];
		bytes memory data = _transaction.data;

		if(to == address(DEPLOYER_SYSTEM_CONTRACT)) {
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
				success := call(gas(), to, value, add(data, 0x20), mload(data), 0, 0)
			}
			require(success);
		}
	}

	// Here, the user pays the bootloader for the transaction
	function payForTransaction(bytes32, bytes32, Transaction calldata _transaction) external payable {
		bool success = _transaction.payToTheBootloader();
		require(success, "Failed to pay the fee to the operator");
	}

	// Here, the user should prepare for the transaction to be paid for by a paymaster
	// Here, the account should set the allowance for the smart contracts
	function prepareForPaymaster(bytes32, bytes32, Transaction calldata _transaction) external payable {
		_transaction.processPaymasterInput();
	}

	fallback() external payable {
		// fallback of default AA shouldn't be called by bootloader under no circumstances 
		assert(msg.sender != BOOTLOADER_FORMAL_ADDRESS);		
		
		// If the contract is called directly, behave like an EOA
	}

	receive() external payable {}
}

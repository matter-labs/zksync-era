// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

import "./Constants.sol";
import "./TransactionHelper.sol";

import "./SystemContractsCaller.sol";

import "./interfaces/IAccount.sol";

contract ValidationRuleBreaker is IAccount {
    using TransactionHelper for Transaction;

    uint public typeOfRuleBreak;
    address public trustedAddress;
    MockToken public mockToken;

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
            require(BOOTLOADER_FORMAL_ADDRESS.balance == 0);
        } else if (typeOfRuleBreak == 4) {
            // This should still fail; EIP-4337 defines out of gas as an immediate failure
            address(this).call(abi.encodeWithSignature("_runOutOfGasRecursively()"));
        } else if (typeOfRuleBreak == 5) {
            runOutOfGasPlain();
        } else if (typeOfRuleBreak == 6) {
            try this.runOutOfGasPlain() {
                revert("Should've run out of gas");
            } catch Panic(uint) {
                // Swallow the error
            }
        } else if (typeOfRuleBreak == 7) {
            // Read storage of another contract via mapping. This should succeed.
            require(mockToken.getBalance(address(this)) == 0);
            require(mockToken.getAllowance(address(this), address(0x800a)) == 0);
            require(mockToken.getAllowance(address(0x10203), address(this)) == 0);
            (uint256 depositCount, uint256 withdrawalCount) = mockToken.getStats(address(this));
            require(depositCount == 0 && withdrawalCount == 0);
            // Offset 100 is acceptable
            require(mockToken.readAfterStats(address(this), 100) == 0);
        } else if (typeOfRuleBreak == 8) {
            // Disallowed read from a layered mapping.
            require(mockToken.getAllowance(address(0x10203), address(0x30201)) == 0);
        } else if (typeOfRuleBreak == 9) {
            // Disallowed read from a mapping with an offset
            (uint256 depositCount, uint256 withdrawalCount) = mockToken.getStats(address(0x10203));
            require(depositCount == 0 && withdrawalCount == 0);
        } else if (typeOfRuleBreak == 10) {
            // Offset 1000 is too large to be recognized as belonging to an allowed mapping entry.
            require(mockToken.readAfterStats(address(this), 1000) == 0);
        }

        _validateTransaction(_suggestedSignedTxHash, _transaction);
    }

    function _runOutOfGasRecursively() external {
        address(this).call(abi.encodeWithSignature("_runOutOfGasRecursively()"));
    }

    /// Runs out of gas w/o recursion.
    function runOutOfGasPlain() public {
        bytes32 value = 0x0000000000000000000000000000000000000000000000000000000000000001;
        while (value != bytes32(0)) {
            value = keccak256(abi.encode(value));
        }
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

contract MockToken {
    // Simple mapping.
    mapping(address => uint256) balances;
    // Layered mapping.
    mapping(address => mapping(address => uint256)) allowances;

    struct AccountStats {
        uint256 depositCount;
        uint256 withdrawalCount;
    }

    // Mapping with values occupying >1 slot.
    mapping(address => AccountStats) stats;

    function getBalance(address _address) external returns (uint256) {
        return balances[_address];
    }

    function getAllowance(address _address, address _recipient) external returns (uint256) {
        return allowances[_address][_recipient];
    }

    function getStats(address _address) external returns (uint256 depositCount, uint256 withdrawalCount) {
        AccountStats storage accountStats = stats[_address];
        depositCount = accountStats.depositCount;
        withdrawalCount = accountStats.withdrawalCount;
    }

    function readAfterStats(address _address, uint256 _offset) external returns (uint256 result) {
        AccountStats storage accountStats = stats[_address];
        assembly {
            result := sload(add(accountStats.slot, _offset))
        }
    }
}

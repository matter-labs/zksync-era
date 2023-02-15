// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

import "./interfaces/IPaymaster.sol";
import "./interfaces/IPaymasterFlow.sol";
import "./TransactionHelper.sol";
import "./Constants.sol";

// This is a dummy paymaster. It expects the paymasterInput to contain its "signature" as well as the needed exchange rate.
// It supports only approval-based paymaster flow.
contract CustomPaymaster is IPaymaster {
    using TransactionHelper for Transaction;

    uint256 public txCounter = 0;
    mapping(uint256 => bool) public calledContext;
    uint256 public wasAnytime = 0;

    bytes32 lastTxHash = 0;

    function validateSignature(bytes memory _signature) internal pure {
        // For the purpose of this test, any signature of length 46 is fine.
        require(_signature.length == 46);
    }

    function validateAndPayForPaymasterTransaction(bytes32 _txHash, bytes32, Transaction calldata _transaction) override external payable returns (bytes4 magic, bytes memory context) {
        // By default we consider the transaction as passed
        magic = PAYMASTER_VALIDATION_SUCCESS_MAGIC;

        lastTxHash = _txHash;
        require(_transaction.paymasterInput.length >= 4, "The standard paymaster input must be at least 4 bytes long");
        
        bytes4 paymasterInputSelector = bytes4(_transaction.paymasterInput[0:4]); 
        if (paymasterInputSelector == IPaymasterFlow.approvalBased.selector) {
            // While the actual data consists of address, uint256 and bytes data, 
            // the data is needed only for the paymaster, so we ignore it here for the sake of optimization
            (address token,, bytes memory input) = abi.decode(_transaction.paymasterInput[4:], (address, uint256, bytes));

            (bytes memory pseudoSignature, uint256 rateNumerator, uint256 rateDenominator, uint256 amount) = abi.decode(input, (bytes, uint256, uint256, uint256));
            validateSignature(pseudoSignature);

            // Firstly, we verify that the user has provided enough allowance
            address userAddress = address(uint160(_transaction.from));
            address thisAddress = address(this);

            uint256 providedAllowance = IERC20(token).allowance(userAddress, thisAddress);
            require(providedAllowance >= amount, "The user did not provide enough allowance");

            uint256 requiredETH = _transaction.gasLimit * _transaction.maxFeePerGas;
            uint256 ethExchnaged = amount * rateNumerator / rateDenominator;

            if (ethExchnaged < requiredETH) {
                // Important note: while this clause definitely means that the user
                // has underpaid the paymaster and the transaction should not accepted,
                // we do not want the transaction to revert, because for fee estimation 
                // we allow users to provide smaller amount of funds then necessary to preserve
                // the property that if using X gas the transaction success, then it will succeed with X+1 gas.
                magic = bytes4(0);
            }

            // Pulling all the tokens from the user 
            IERC20(token).transferFrom(userAddress, thisAddress, amount);
            bool success = _transaction.payToTheBootloader();
            require(success, "Failed to transfer funds to the bootloader");

            // For now, refunds are not supported, so we just test the fact that the transfered context is correct
            txCounter += 1;
            context = abi.encode(txCounter);
        } else {
            revert("Unsupported paymaster flow");
        }
    }

    function postTransaction(
        bytes calldata _context,
        Transaction calldata,
        bytes32 _txHash,
        bytes32, 
        ExecutionResult,
        uint256
    ) override external payable {
        require(_txHash == lastTxHash, "Incorrect last tx hash");
        uint256 contextCounter = abi.decode(_context, (uint256));
        calledContext[contextCounter] = true;
    }

    receive() external payable {}
}

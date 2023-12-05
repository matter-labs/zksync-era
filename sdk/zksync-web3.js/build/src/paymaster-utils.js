"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.getPaymasterParams = exports.getGeneralPaymasterInput = exports.getApprovalBasedPaymasterInput = exports.IPaymasterFlow = void 0;
const ethers_1 = require("ethers");
exports.IPaymasterFlow = new ethers_1.ethers.utils.Interface(require('../../abi/IPaymasterFlow.json').abi);
function getApprovalBasedPaymasterInput(paymasterInput) {
    return exports.IPaymasterFlow.encodeFunctionData('approvalBased', [
        paymasterInput.token,
        paymasterInput.minimalAllowance,
        paymasterInput.innerInput
    ]);
}
exports.getApprovalBasedPaymasterInput = getApprovalBasedPaymasterInput;
function getGeneralPaymasterInput(paymasterInput) {
    return exports.IPaymasterFlow.encodeFunctionData('general', [paymasterInput.innerInput]);
}
exports.getGeneralPaymasterInput = getGeneralPaymasterInput;
function getPaymasterParams(paymasterAddress, paymasterInput) {
    if (paymasterInput.type == 'General') {
        return {
            paymaster: paymasterAddress,
            paymasterInput: getGeneralPaymasterInput(paymasterInput)
        };
    }
    else {
        return {
            paymaster: paymasterAddress,
            paymasterInput: getApprovalBasedPaymasterInput(paymasterInput)
        };
    }
}
exports.getPaymasterParams = getPaymasterParams;

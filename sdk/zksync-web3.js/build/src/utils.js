"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __exportStar = (this && this.__exportStar) || function(m, exports) {
    for (var p in m) if (p !== "default" && !Object.prototype.hasOwnProperty.call(exports, p)) __createBinding(exports, m, p);
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.estimateCustomBridgeDepositL2Gas = exports.scaleGasLimit = exports.estimateDefaultBridgeDepositL2Gas = exports.isTypedDataSignatureCorrect = exports.isMessageSignatureCorrect = exports.getERC20BridgeCalldata = exports.getERC20DefaultBridgeData = exports.undoL1ToL2Alias = exports.applyL1ToL2Alias = exports.getL2HashFromPriorityOp = exports.parseTransaction = exports.hashBytecode = exports.serialize = exports.checkBaseCost = exports.createAddress = exports.create2Address = exports.getDeployedContracts = exports.getHashedL2ToL1Msg = exports.layer1TxDefaults = exports.sleep = exports.isETH = exports.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT = exports.DEFAULT_GAS_PER_PUBDATA_LIMIT = exports.L1_RECOMMENDED_MIN_ETH_DEPOSIT_GAS_LIMIT = exports.L1_RECOMMENDED_MIN_ERC20_DEPOSIT_GAS_LIMIT = exports.L1_FEE_ESTIMATION_COEF_DENOMINATOR = exports.L1_FEE_ESTIMATION_COEF_NUMERATOR = exports.MAX_BYTECODE_LEN_BYTES = exports.PRIORITY_OPERATION_L2_TX_TYPE = exports.EIP712_TX_TYPE = exports.EIP1271_MAGIC_VALUE = exports.L1_TO_L2_ALIAS_OFFSET = exports.L2_ETH_TOKEN_ADDRESS = exports.L1_MESSENGER_ADDRESS = exports.CONTRACT_DEPLOYER_ADDRESS = exports.BOOTLOADER_FORMAL_ADDRESS = exports.L2_BRIDGE_ABI = exports.L1_BRIDGE_ABI = exports.IERC1271 = exports.IERC20 = exports.L1_MESSENGER = exports.CONTRACT_DEPLOYER = exports.ZKSYNC_MAIN_ABI = exports.ETH_ADDRESS = void 0;
const ethers_1 = require("ethers");
const types_1 = require("./types");
const signer_1 = require("./signer");
const typechain_1 = require("../typechain");
const utils_1 = require("ethers/lib/utils");
__exportStar(require("./paymaster-utils"), exports);
exports.ETH_ADDRESS = '0x0000000000000000000000000000000000000000';
exports.ZKSYNC_MAIN_ABI = new ethers_1.utils.Interface(require('../../abi/IZkSync.json').abi);
exports.CONTRACT_DEPLOYER = new ethers_1.utils.Interface(require('../../abi/ContractDeployer.json').abi);
exports.L1_MESSENGER = new ethers_1.utils.Interface(require('../../abi/IL1Messenger.json').abi);
exports.IERC20 = new ethers_1.utils.Interface(require('../../abi/IERC20.json').abi);
exports.IERC1271 = new ethers_1.utils.Interface(require('../../abi/IERC1271.json').abi);
exports.L1_BRIDGE_ABI = new ethers_1.utils.Interface(require('../../abi/IL1Bridge.json').abi);
exports.L2_BRIDGE_ABI = new ethers_1.utils.Interface(require('../../abi/IL2Bridge.json').abi);
exports.BOOTLOADER_FORMAL_ADDRESS = '0x0000000000000000000000000000000000008001';
exports.CONTRACT_DEPLOYER_ADDRESS = '0x0000000000000000000000000000000000008006';
exports.L1_MESSENGER_ADDRESS = '0x0000000000000000000000000000000000008008';
exports.L2_ETH_TOKEN_ADDRESS = '0x000000000000000000000000000000000000800a';
exports.L1_TO_L2_ALIAS_OFFSET = '0x1111000000000000000000000000000000001111';
exports.EIP1271_MAGIC_VALUE = '0x1626ba7e';
exports.EIP712_TX_TYPE = 0x71;
exports.PRIORITY_OPERATION_L2_TX_TYPE = 0xff;
exports.MAX_BYTECODE_LEN_BYTES = ((1 << 16) - 1) * 32;
// Currently, for some reason the SDK may return slightly smaller L1 gas limit than required for initiating L1->L2
// transaction. We use a coefficient to ensure that the transaction will be accepted.
exports.L1_FEE_ESTIMATION_COEF_NUMERATOR = ethers_1.BigNumber.from(12);
exports.L1_FEE_ESTIMATION_COEF_DENOMINATOR = ethers_1.BigNumber.from(10);
// This gas limit will be used for displaying the error messages when the users do not have enough fee.
exports.L1_RECOMMENDED_MIN_ERC20_DEPOSIT_GAS_LIMIT = 400000;
exports.L1_RECOMMENDED_MIN_ETH_DEPOSIT_GAS_LIMIT = 200000;
// The large L2 gas per pubdata to sign. This gas is enough to ensure that
// any reasonable limit will be accepted. Note, that the operator is NOT required to
// use the honest value of gas per pubdata and it can use any value up to the one signed by the user.
// In the future releases, we will provide a way to estimate the current gasPerPubdata.
exports.DEFAULT_GAS_PER_PUBDATA_LIMIT = 50000;
// It is possible to provide practically any gasPerPubdataByte for L1->L2 transactions, since
// the cost per gas will be adjusted respectively. We will use 800 as an relatively optimal value for now.
exports.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT = 800;
function isETH(token) {
    return token.toLowerCase() == exports.ETH_ADDRESS || token.toLowerCase() == exports.L2_ETH_TOKEN_ADDRESS;
}
exports.isETH = isETH;
function sleep(millis) {
    return new Promise((resolve) => setTimeout(resolve, millis));
}
exports.sleep = sleep;
function layer1TxDefaults() {
    return {
        queueType: types_1.PriorityQueueType.Deque,
        opTree: types_1.PriorityOpTree.Full
    };
}
exports.layer1TxDefaults = layer1TxDefaults;
function getHashedL2ToL1Msg(sender, msg, txNumberInBlock) {
    const encodedMsg = new Uint8Array([
        0,
        1,
        ...ethers_1.ethers.utils.zeroPad(ethers_1.ethers.utils.hexlify(txNumberInBlock), 2),
        ...ethers_1.ethers.utils.arrayify(exports.L1_MESSENGER_ADDRESS),
        ...ethers_1.ethers.utils.zeroPad(sender, 32),
        ...ethers_1.ethers.utils.arrayify(ethers_1.ethers.utils.keccak256(msg))
    ]);
    return ethers_1.ethers.utils.keccak256(encodedMsg);
}
exports.getHashedL2ToL1Msg = getHashedL2ToL1Msg;
function getDeployedContracts(receipt) {
    const addressBytesLen = 40;
    const deployedContracts = receipt.logs
        .filter((log) => log.topics[0] == ethers_1.utils.id('ContractDeployed(address,bytes32,address)') &&
        log.address == exports.CONTRACT_DEPLOYER_ADDRESS)
        // Take the last topic (deployed contract address as U256) and extract address from it (U160).
        .map((log) => {
        const sender = `0x${log.topics[1].slice(log.topics[1].length - addressBytesLen)}`;
        const bytesCodehash = log.topics[2];
        const address = `0x${log.topics[3].slice(log.topics[3].length - addressBytesLen)}`;
        return {
            sender: ethers_1.utils.getAddress(sender),
            bytecodeHash: bytesCodehash,
            deployedAddress: ethers_1.utils.getAddress(address)
        };
    });
    return deployedContracts;
}
exports.getDeployedContracts = getDeployedContracts;
function create2Address(sender, bytecodeHash, salt, input) {
    const prefix = ethers_1.ethers.utils.keccak256(ethers_1.ethers.utils.toUtf8Bytes('zksyncCreate2'));
    const inputHash = ethers_1.ethers.utils.keccak256(input);
    const addressBytes = ethers_1.ethers.utils
        .keccak256(ethers_1.ethers.utils.concat([prefix, ethers_1.ethers.utils.zeroPad(sender, 32), salt, bytecodeHash, inputHash]))
        .slice(26);
    return ethers_1.ethers.utils.getAddress(addressBytes);
}
exports.create2Address = create2Address;
function createAddress(sender, senderNonce) {
    const prefix = ethers_1.ethers.utils.keccak256(ethers_1.ethers.utils.toUtf8Bytes('zksyncCreate'));
    const addressBytes = ethers_1.ethers.utils
        .keccak256(ethers_1.ethers.utils.concat([
        prefix,
        ethers_1.ethers.utils.zeroPad(sender, 32),
        ethers_1.ethers.utils.zeroPad(ethers_1.ethers.utils.hexlify(senderNonce), 32)
    ]))
        .slice(26);
    return ethers_1.ethers.utils.getAddress(addressBytes);
}
exports.createAddress = createAddress;
async function checkBaseCost(baseCost, value) {
    if (baseCost.gt(await value)) {
        throw new Error(`The base cost of performing the priority operation is higher than the provided value parameter ` +
            `for the transaction: baseCost: ${baseCost}, provided value: ${value}`);
    }
}
exports.checkBaseCost = checkBaseCost;
function serialize(transaction, signature) {
    var _a;
    if (transaction.customData == null && transaction.type != exports.EIP712_TX_TYPE) {
        return ethers_1.utils.serializeTransaction(transaction, signature);
    }
    if (!transaction.chainId) {
        throw Error("Transaction chainId isn't set");
    }
    function formatNumber(value, name) {
        const result = ethers_1.utils.stripZeros(ethers_1.BigNumber.from(value).toHexString());
        if (result.length > 32) {
            throw new Error('invalid length for ' + name);
        }
        return result;
    }
    if (!transaction.from) {
        throw new Error('Explicitly providing `from` field is reqiured for EIP712 transactions');
    }
    const from = transaction.from;
    const meta = transaction.customData;
    let maxFeePerGas = transaction.maxFeePerGas || transaction.gasPrice || 0;
    let maxPriorityFeePerGas = transaction.maxPriorityFeePerGas || maxFeePerGas;
    const fields = [
        formatNumber(transaction.nonce || 0, 'nonce'),
        formatNumber(maxPriorityFeePerGas, 'maxPriorityFeePerGas'),
        formatNumber(maxFeePerGas, 'maxFeePerGas'),
        formatNumber(transaction.gasLimit || 0, 'gasLimit'),
        transaction.to != null ? ethers_1.utils.getAddress(transaction.to) : '0x',
        formatNumber(transaction.value || 0, 'value'),
        transaction.data || '0x'
    ];
    if (signature) {
        const sig = ethers_1.utils.splitSignature(signature);
        fields.push(formatNumber(sig.recoveryParam, 'recoveryParam'));
        fields.push(ethers_1.utils.stripZeros(sig.r));
        fields.push(ethers_1.utils.stripZeros(sig.s));
    }
    else {
        fields.push(formatNumber(transaction.chainId, 'chainId'));
        fields.push('0x');
        fields.push('0x');
    }
    fields.push(formatNumber(transaction.chainId, 'chainId'));
    fields.push(ethers_1.utils.getAddress(from));
    // Add meta
    fields.push(formatNumber(meta.gasPerPubdata || exports.DEFAULT_GAS_PER_PUBDATA_LIMIT, 'gasPerPubdata'));
    fields.push(((_a = meta.factoryDeps) !== null && _a !== void 0 ? _a : []).map((dep) => ethers_1.utils.hexlify(dep)));
    if (meta.customSignature && ethers_1.ethers.utils.arrayify(meta.customSignature).length == 0) {
        throw new Error('Empty signatures are not supported');
    }
    fields.push(meta.customSignature || '0x');
    if (meta.paymasterParams) {
        fields.push([meta.paymasterParams.paymaster, ethers_1.ethers.utils.hexlify(meta.paymasterParams.paymasterInput)]);
    }
    else {
        fields.push([]);
    }
    return ethers_1.utils.hexConcat([[exports.EIP712_TX_TYPE], ethers_1.utils.RLP.encode(fields)]);
}
exports.serialize = serialize;
function hashBytecode(bytecode) {
    // For getting the consistent length we first convert the bytecode to UInt8Array
    const bytecodeAsArray = ethers_1.ethers.utils.arrayify(bytecode);
    if (bytecodeAsArray.length % 32 != 0) {
        throw new Error('The bytecode length in bytes must be divisible by 32');
    }
    if (bytecodeAsArray.length > exports.MAX_BYTECODE_LEN_BYTES) {
        throw new Error(`Bytecode can not be longer than ${exports.MAX_BYTECODE_LEN_BYTES} bytes`);
    }
    const hashStr = ethers_1.ethers.utils.sha256(bytecodeAsArray);
    const hash = ethers_1.ethers.utils.arrayify(hashStr);
    // Note that the length of the bytecode
    // should be provided in 32-byte words.
    const bytecodeLengthInWords = bytecodeAsArray.length / 32;
    if (bytecodeLengthInWords % 2 == 0) {
        throw new Error('Bytecode length in 32-byte words must be odd');
    }
    const bytecodeLength = ethers_1.ethers.utils.arrayify(bytecodeLengthInWords);
    // The bytecode should always take the first 2 bytes of the bytecode hash,
    // so we pad it from the left in case the length is smaller than 2 bytes.
    const bytecodeLengthPadded = ethers_1.ethers.utils.zeroPad(bytecodeLength, 2);
    const codeHashVersion = new Uint8Array([1, 0]);
    hash.set(codeHashVersion, 0);
    hash.set(bytecodeLengthPadded, 2);
    return hash;
}
exports.hashBytecode = hashBytecode;
function parseTransaction(payload) {
    function handleAddress(value) {
        if (value === '0x') {
            return null;
        }
        return ethers_1.utils.getAddress(value);
    }
    function handleNumber(value) {
        if (value === '0x') {
            return ethers_1.BigNumber.from(0);
        }
        return ethers_1.BigNumber.from(value);
    }
    function arrayToPaymasterParams(arr) {
        if (arr.length == 0) {
            return undefined;
        }
        if (arr.length != 2) {
            throw new Error(`Invalid paymaster parameters, expected to have length of 2, found ${arr.length}`);
        }
        return {
            paymaster: ethers_1.utils.getAddress(arr[0]),
            paymasterInput: ethers_1.utils.arrayify(arr[1])
        };
    }
    const bytes = ethers_1.utils.arrayify(payload);
    if (bytes[0] != exports.EIP712_TX_TYPE) {
        return ethers_1.utils.parseTransaction(bytes);
    }
    const raw = ethers_1.utils.RLP.decode(bytes.slice(1));
    const transaction = {
        type: exports.EIP712_TX_TYPE,
        nonce: handleNumber(raw[0]).toNumber(),
        maxPriorityFeePerGas: handleNumber(raw[1]),
        maxFeePerGas: handleNumber(raw[2]),
        gasLimit: handleNumber(raw[3]),
        to: handleAddress(raw[4]),
        value: handleNumber(raw[5]),
        data: raw[6],
        chainId: handleNumber(raw[10]),
        from: handleAddress(raw[11]),
        customData: {
            gasPerPubdata: handleNumber(raw[12]),
            factoryDeps: raw[13],
            customSignature: raw[14],
            paymasterParams: arrayToPaymasterParams(raw[15])
        }
    };
    const ethSignature = {
        v: handleNumber(raw[7]).toNumber(),
        r: raw[8],
        s: raw[9]
    };
    if ((ethers_1.utils.hexlify(ethSignature.r) == '0x' || ethers_1.utils.hexlify(ethSignature.s) == '0x') &&
        !transaction.customData.customSignature) {
        return transaction;
    }
    if (ethSignature.v !== 0 && ethSignature.v !== 1 && !transaction.customData.customSignature) {
        throw new Error('Failed to parse signature');
    }
    if (!transaction.customData.customSignature) {
        transaction.v = ethSignature.v;
        transaction.s = ethSignature.s;
        transaction.r = ethSignature.r;
    }
    transaction.hash = eip712TxHash(transaction, ethSignature);
    return transaction;
}
exports.parseTransaction = parseTransaction;
function getSignature(transaction, ethSignature) {
    var _a;
    if (((_a = transaction === null || transaction === void 0 ? void 0 : transaction.customData) === null || _a === void 0 ? void 0 : _a.customSignature) && transaction.customData.customSignature.length) {
        return ethers_1.ethers.utils.arrayify(transaction.customData.customSignature);
    }
    if (!ethSignature) {
        throw new Error('No signature provided');
    }
    const r = ethers_1.ethers.utils.zeroPad(ethers_1.ethers.utils.arrayify(ethSignature.r), 32);
    const s = ethers_1.ethers.utils.zeroPad(ethers_1.ethers.utils.arrayify(ethSignature.s), 32);
    const v = ethSignature.v;
    return new Uint8Array([...r, ...s, v]);
}
function eip712TxHash(transaction, ethSignature) {
    const signedDigest = signer_1.EIP712Signer.getSignedDigest(transaction);
    const hashedSignature = ethers_1.ethers.utils.keccak256(getSignature(transaction, ethSignature));
    return ethers_1.ethers.utils.keccak256(ethers_1.ethers.utils.hexConcat([signedDigest, hashedSignature]));
}
function getL2HashFromPriorityOp(txReceipt, zkSyncAddress) {
    let txHash = null;
    for (const log of txReceipt.logs) {
        if (log.address.toLowerCase() != zkSyncAddress.toLowerCase()) {
            continue;
        }
        try {
            const priorityQueueLog = exports.ZKSYNC_MAIN_ABI.parseLog(log);
            if (priorityQueueLog && priorityQueueLog.args.txHash != null) {
                txHash = priorityQueueLog.args.txHash;
            }
        }
        catch { }
    }
    if (!txHash) {
        throw new Error('Failed to parse tx logs');
    }
    return txHash;
}
exports.getL2HashFromPriorityOp = getL2HashFromPriorityOp;
const ADDRESS_MODULO = ethers_1.BigNumber.from(2).pow(160);
function applyL1ToL2Alias(address) {
    return ethers_1.ethers.utils.hexlify(ethers_1.ethers.BigNumber.from(address).add(exports.L1_TO_L2_ALIAS_OFFSET).mod(ADDRESS_MODULO));
}
exports.applyL1ToL2Alias = applyL1ToL2Alias;
function undoL1ToL2Alias(address) {
    let result = ethers_1.ethers.BigNumber.from(address).sub(exports.L1_TO_L2_ALIAS_OFFSET);
    if (result.lt(ethers_1.BigNumber.from(0))) {
        result = result.add(ADDRESS_MODULO);
    }
    return ethers_1.ethers.utils.hexlify(result);
}
exports.undoL1ToL2Alias = undoL1ToL2Alias;
/// Getters data used to correctly initialize the L1 token counterpart on L2
async function getERC20DefaultBridgeData(l1TokenAddress, provider) {
    const token = typechain_1.IERC20MetadataFactory.connect(l1TokenAddress, provider);
    const name = await token.name();
    const symbol = await token.symbol();
    const decimals = await token.decimals();
    const coder = new utils_1.AbiCoder();
    const nameBytes = coder.encode(['string'], [name]);
    const symbolBytes = coder.encode(['string'], [symbol]);
    const decimalsBytes = coder.encode(['uint256'], [decimals]);
    return coder.encode(['bytes', 'bytes', 'bytes'], [nameBytes, symbolBytes, decimalsBytes]);
}
exports.getERC20DefaultBridgeData = getERC20DefaultBridgeData;
/// The method that returns the calldata that will be sent by an L1 ERC20 bridge to its L2 counterpart
/// during bridging of a token.
async function getERC20BridgeCalldata(l1TokenAddress, l1Sender, l2Receiver, amount, bridgeData) {
    return exports.L2_BRIDGE_ABI.encodeFunctionData('finalizeDeposit', [
        l1Sender,
        l2Receiver,
        l1TokenAddress,
        amount,
        bridgeData
    ]);
}
exports.getERC20BridgeCalldata = getERC20BridgeCalldata;
// The method with similar functionality is already available in ethers.js,
// the only difference is that we provide additional `try { } catch { }`
// for error-resilience.
//
// It will also pave the road for allowing future EIP-1271 signature verification, by
// letting our SDK have functionality to verify signatures.
function isECDSASignatureCorrect(address, msgHash, signature) {
    try {
        return address == ethers_1.ethers.utils.recoverAddress(msgHash, signature);
    }
    catch {
        // In case ECDSA signature verification has thrown an error,
        // we simply consider the signature as incorrect.
        return false;
    }
}
async function isEIP1271SignatureCorrect(provider, address, msgHash, signature) {
    const accountContract = new ethers_1.ethers.Contract(address, exports.IERC1271, provider);
    // This line may throw an exception if the contract does not implement the EIP1271 correctly.
    // But it may also throw an exception in case the internet connection is lost.
    // It is the caller's responsibility to handle the exception.
    const result = await accountContract.isValidSignature(msgHash, signature);
    return result == exports.EIP1271_MAGIC_VALUE;
}
async function isSignatureCorrect(provider, address, msgHash, signature) {
    let isContractAccount = false;
    const code = await provider.getCode(address);
    isContractAccount = ethers_1.ethers.utils.arrayify(code).length != 0;
    if (!isContractAccount) {
        return isECDSASignatureCorrect(address, msgHash, signature);
    }
    else {
        return await isEIP1271SignatureCorrect(provider, address, msgHash, signature);
    }
}
// Returns `true` or `false` depending on whether or not the account abstraction's
// signature is correct. Note, that while currently it does not do any `async` actions.
// in the future it will. That's why the `Promise<boolean>` is returned.
async function isMessageSignatureCorrect(provider, address, message, signature) {
    const msgHash = ethers_1.ethers.utils.hashMessage(message);
    return await isSignatureCorrect(provider, address, msgHash, signature);
}
exports.isMessageSignatureCorrect = isMessageSignatureCorrect;
// Returns `true` or `false` depending on whether or not the account abstraction's
// EIP712 signature is correct. Note, that while currently it does not do any `async` actions.
// in the future it will. That's why the `Promise<boolean>` is returned.
async function isTypedDataSignatureCorrect(provider, address, domain, types, value, signature) {
    const msgHash = ethers_1.ethers.utils._TypedDataEncoder.hash(domain, types, value);
    return await isSignatureCorrect(provider, address, msgHash, signature);
}
exports.isTypedDataSignatureCorrect = isTypedDataSignatureCorrect;
async function estimateDefaultBridgeDepositL2Gas(providerL1, providerL2, token, amount, to, from, gasPerPubdataByte) {
    // If the `from` address is not provided, we use a random address, because
    // due to storage slot aggregation, the gas estimation will depend on the address
    // and so estimation for the zero address may be smaller than for the sender.
    from !== null && from !== void 0 ? from : (from = ethers_1.ethers.Wallet.createRandom().address);
    if (token == exports.ETH_ADDRESS) {
        return await providerL2.estimateL1ToL2Execute({
            contractAddress: to,
            gasPerPubdataByte: gasPerPubdataByte,
            caller: from,
            calldata: '0x',
            l2Value: amount
        });
    }
    else {
        let value, l1BridgeAddress, l2BridgeAddress, bridgeData;
        const bridgeAddresses = await providerL2.getDefaultBridgeAddresses();
        const l1WethBridge = typechain_1.IL1BridgeFactory.connect(bridgeAddresses.wethL1, providerL1);
        const l2WethToken = await l1WethBridge.l2TokenAddress(token);
        if (l2WethToken != ethers_1.ethers.constants.AddressZero) {
            value = amount;
            l1BridgeAddress = bridgeAddresses.wethL1;
            l2BridgeAddress = bridgeAddresses.wethL2;
            bridgeData = '0x';
        }
        else {
            value = 0;
            l1BridgeAddress = bridgeAddresses.erc20L1;
            l2BridgeAddress = bridgeAddresses.erc20L2;
            bridgeData = await getERC20DefaultBridgeData(token, providerL1);
        }
        return await estimateCustomBridgeDepositL2Gas(providerL2, l1BridgeAddress, l2BridgeAddress, token, amount, to, bridgeData, from, gasPerPubdataByte, value);
    }
}
exports.estimateDefaultBridgeDepositL2Gas = estimateDefaultBridgeDepositL2Gas;
function scaleGasLimit(gasLimit) {
    return gasLimit.mul(exports.L1_FEE_ESTIMATION_COEF_NUMERATOR).div(exports.L1_FEE_ESTIMATION_COEF_DENOMINATOR);
}
exports.scaleGasLimit = scaleGasLimit;
async function estimateCustomBridgeDepositL2Gas(providerL2, l1BridgeAddress, l2BridgeAddress, token, amount, to, bridgeData, from, gasPerPubdataByte, l2Value) {
    const calldata = await getERC20BridgeCalldata(token, from, to, amount, bridgeData);
    return await providerL2.estimateL1ToL2Execute({
        caller: applyL1ToL2Alias(l1BridgeAddress),
        contractAddress: l2BridgeAddress,
        gasPerPubdataByte,
        calldata,
        l2Value
    });
}
exports.estimateCustomBridgeDepositL2Gas = estimateCustomBridgeDepositL2Gas;

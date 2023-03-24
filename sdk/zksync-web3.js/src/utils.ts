import { utils, ethers, BigNumber, BigNumberish, BytesLike } from 'ethers';
import { SignatureLike } from '@ethersproject/bytes';
import {
    Address,
    Eip712Meta,
    PriorityQueueType,
    PriorityOpTree,
    DeploymentInfo,
    PaymasterParams,
    EthereumSignature
} from './types';
import { TypedDataDomain, TypedDataField } from '@ethersproject/abstract-signer';
import { Provider } from './provider';
import { EIP712Signer } from './signer';
import { IERC20MetadataFactory } from '../typechain';
import { AbiCoder } from 'ethers/lib/utils';

export * from './paymaster-utils';

export const ETH_ADDRESS = '0x0000000000000000000000000000000000000000';

export const ZKSYNC_MAIN_ABI = new utils.Interface(require('../../abi/IZkSync.json').abi);
export const CONTRACT_DEPLOYER = new utils.Interface(require('../../abi/ContractDeployer.json').abi);
export const L1_MESSENGER = new utils.Interface(require('../../abi/IL1Messenger.json').abi);
export const IERC20 = new utils.Interface(require('../../abi/IERC20.json').abi);
export const IERC1271 = new utils.Interface(require('../../abi/IERC1271.json').abi);
export const L1_BRIDGE_ABI = new utils.Interface(require('../../abi/IL1Bridge.json').abi);
export const L2_BRIDGE_ABI = new utils.Interface(require('../../abi/IL2Bridge.json').abi);

export const BOOTLOADER_FORMAL_ADDRESS = '0x0000000000000000000000000000000000008001';
export const CONTRACT_DEPLOYER_ADDRESS = '0x0000000000000000000000000000000000008006';
export const L1_MESSENGER_ADDRESS = '0x0000000000000000000000000000000000008008';
export const L2_ETH_TOKEN_ADDRESS = '0x000000000000000000000000000000000000800a';

export const L1_TO_L2_ALIAS_OFFSET = '0x1111000000000000000000000000000000001111';

export const EIP1271_MAGIC_VALUE = '0x1626ba7e';

export const EIP712_TX_TYPE = 0x71;
export const PRIORITY_OPERATION_L2_TX_TYPE = 0xff;

export const MAX_BYTECODE_LEN_BYTES = ((1 << 16) - 1) * 32;

// The large L2 gas per pubdata to sign. This gas is enough to ensure that
// any reasonable limit will be accepted. Note, that the operator is NOT required to
// use the honest value of gas per pubdata and it can use any value up to the one signed by the user.
// In the future releases, we will provide a way to estimate the current gasPerPubdata.
export const DEFAULT_GAS_PER_PUBDATA_LIMIT = 50000;

// It is possible to provide practically any gasPerPubdataByte for L1->L2 transactions, since
// the cost per gas will be adjusted respectively. We will use 800 as an relatively optimal value for now.
export const REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT = 800;

export function isETH(token: Address) {
    return token.toLowerCase() == ETH_ADDRESS || token.toLowerCase() == L2_ETH_TOKEN_ADDRESS;
}

export function sleep(millis: number) {
    return new Promise((resolve) => setTimeout(resolve, millis));
}

export function layer1TxDefaults() {
    return {
        queueType: PriorityQueueType.Deque,
        opTree: PriorityOpTree.Full
    };
}

export function getHashedL2ToL1Msg(sender: Address, msg: BytesLike, txNumberInBlock: number) {
    const encodedMsg = new Uint8Array([
        0, // l2ShardId
        1, // isService
        ...ethers.utils.zeroPad(ethers.utils.hexlify(txNumberInBlock), 2),
        ...ethers.utils.arrayify(L1_MESSENGER_ADDRESS),
        ...ethers.utils.zeroPad(sender, 32),
        ...ethers.utils.arrayify(ethers.utils.keccak256(msg))
    ]);

    return ethers.utils.keccak256(encodedMsg);
}

export function getDeployedContracts(receipt: ethers.providers.TransactionReceipt): DeploymentInfo[] {
    const addressBytesLen = 40;
    const deployedContracts = receipt.logs
        .filter(
            (log) =>
                log.topics[0] == utils.id('ContractDeployed(address,bytes32,address)') &&
                log.address == CONTRACT_DEPLOYER_ADDRESS
        )
        // Take the last topic (deployed contract address as U256) and extract address from it (U160).
        .map((log) => {
            const sender = `0x${log.topics[1].slice(log.topics[1].length - addressBytesLen)}`;
            const bytesCodehash = log.topics[2];
            const address = `0x${log.topics[3].slice(log.topics[3].length - addressBytesLen)}`;
            return {
                sender: utils.getAddress(sender),
                bytecodeHash: bytesCodehash,
                deployedAddress: utils.getAddress(address)
            };
        });

    return deployedContracts;
}

export function create2Address(sender: Address, bytecodeHash: BytesLike, salt: BytesLike, input: BytesLike) {
    const prefix = ethers.utils.keccak256(ethers.utils.toUtf8Bytes('zksyncCreate2'));
    const inputHash = ethers.utils.keccak256(input);
    const addressBytes = ethers.utils
        .keccak256(ethers.utils.concat([prefix, ethers.utils.zeroPad(sender, 32), salt, bytecodeHash, inputHash]))
        .slice(26);
    return ethers.utils.getAddress(addressBytes);
}

export function createAddress(sender: Address, senderNonce: BigNumberish) {
    const prefix = ethers.utils.keccak256(ethers.utils.toUtf8Bytes('zksyncCreate'));
    const addressBytes = ethers.utils
        .keccak256(
            ethers.utils.concat([
                prefix,
                ethers.utils.zeroPad(sender, 32),
                ethers.utils.zeroPad(ethers.utils.hexlify(senderNonce), 32)
            ])
        )
        .slice(26);

    return ethers.utils.getAddress(addressBytes);
}

export async function checkBaseCost(
    baseCost: ethers.BigNumber,
    value: ethers.BigNumberish | Promise<ethers.BigNumberish>
) {
    if (baseCost.gt(await value)) {
        throw new Error(
            `The base cost of performing the priority operation is higher than the provided value parameter ` +
                `for the transaction: baseCost: ${baseCost}, provided value: ${value}`
        );
    }
}

export function serialize(transaction: ethers.providers.TransactionRequest, signature?: SignatureLike) {
    if (transaction.customData == null && transaction.type != EIP712_TX_TYPE) {
        return utils.serializeTransaction(transaction as ethers.PopulatedTransaction, signature);
    }
    if (!transaction.chainId) {
        throw Error("Transaction chainId isn't set");
    }

    function formatNumber(value: BigNumberish, name: string): Uint8Array {
        const result = utils.stripZeros(BigNumber.from(value).toHexString());
        if (result.length > 32) {
            throw new Error('invalid length for ' + name);
        }
        return result;
    }

    if (!transaction.from) {
        throw new Error('Explicitly providing `from` field is reqiured for EIP712 transactions');
    }
    const from = transaction.from;

    const meta: Eip712Meta = transaction.customData;

    let maxFeePerGas = transaction.maxFeePerGas || transaction.gasPrice || 0;
    let maxPriorityFeePerGas = transaction.maxPriorityFeePerGas || maxFeePerGas;

    const fields: any[] = [
        formatNumber(transaction.nonce || 0, 'nonce'),
        formatNumber(maxPriorityFeePerGas, 'maxPriorityFeePerGas'),
        formatNumber(maxFeePerGas, 'maxFeePerGas'),
        formatNumber(transaction.gasLimit || 0, 'gasLimit'),
        transaction.to != null ? utils.getAddress(transaction.to) : '0x',
        formatNumber(transaction.value || 0, 'value'),
        transaction.data || '0x'
    ];

    if (signature) {
        const sig = utils.splitSignature(signature);
        fields.push(formatNumber(sig.recoveryParam, 'recoveryParam'));
        fields.push(utils.stripZeros(sig.r));
        fields.push(utils.stripZeros(sig.s));
    } else {
        fields.push(formatNumber(transaction.chainId, 'chainId'));
        fields.push('0x');
        fields.push('0x');
    }
    fields.push(formatNumber(transaction.chainId, 'chainId'));
    fields.push(utils.getAddress(from));

    // Add meta
    fields.push(formatNumber(meta.gasPerPubdata || DEFAULT_GAS_PER_PUBDATA_LIMIT, 'gasPerPubdata'));
    fields.push((meta.factoryDeps ?? []).map((dep) => utils.hexlify(dep)));

    if (meta.customSignature && ethers.utils.arrayify(meta.customSignature).length == 0) {
        throw new Error('Empty signatures are not supported');
    }
    fields.push(meta.customSignature || '0x');

    if (meta.paymasterParams) {
        fields.push([meta.paymasterParams.paymaster, ethers.utils.hexlify(meta.paymasterParams.paymasterInput)]);
    } else {
        fields.push([]);
    }

    return utils.hexConcat([[EIP712_TX_TYPE], utils.RLP.encode(fields)]);
}

export function hashBytecode(bytecode: ethers.BytesLike): Uint8Array {
    // For getting the consistent length we first convert the bytecode to UInt8Array
    const bytecodeAsArray = ethers.utils.arrayify(bytecode);

    if (bytecodeAsArray.length % 32 != 0) {
        throw new Error('The bytecode length in bytes must be divisible by 32');
    }

    if (bytecodeAsArray.length > MAX_BYTECODE_LEN_BYTES) {
        throw new Error(`Bytecode can not be longer than ${MAX_BYTECODE_LEN_BYTES} bytes`);
    }

    const hashStr = ethers.utils.sha256(bytecodeAsArray);
    const hash = ethers.utils.arrayify(hashStr);

    // Note that the length of the bytecode
    // should be provided in 32-byte words.
    const bytecodeLengthInWords = bytecodeAsArray.length / 32;
    if (bytecodeLengthInWords % 2 == 0) {
        throw new Error('Bytecode length in 32-byte words must be odd');
    }

    const bytecodeLength = ethers.utils.arrayify(bytecodeLengthInWords);

    // The bytecode should always take the first 2 bytes of the bytecode hash,
    // so we pad it from the left in case the length is smaller than 2 bytes.
    const bytecodeLengthPadded = ethers.utils.zeroPad(bytecodeLength, 2);

    const codeHashVersion = new Uint8Array([1, 0]);
    hash.set(codeHashVersion, 0);
    hash.set(bytecodeLengthPadded, 2);

    return hash;
}

export function parseTransaction(payload: ethers.BytesLike): ethers.Transaction {
    function handleAddress(value: string): string {
        if (value === '0x') {
            return null;
        }
        return utils.getAddress(value);
    }

    function handleNumber(value: string): BigNumber {
        if (value === '0x') {
            return BigNumber.from(0);
        }
        return BigNumber.from(value);
    }

    function arrayToPaymasterParams(arr: string[]): PaymasterParams | undefined {
        if (arr.length == 0) {
            return undefined;
        }
        if (arr.length != 2) {
            throw new Error(`Invalid paymaster parameters, expected to have length of 2, found ${arr.length}`);
        }

        return {
            paymaster: utils.getAddress(arr[0]),
            paymasterInput: utils.arrayify(arr[1])
        };
    }

    const bytes = utils.arrayify(payload);
    if (bytes[0] != EIP712_TX_TYPE) {
        return utils.parseTransaction(bytes);
    }

    const raw = utils.RLP.decode(bytes.slice(1));
    const transaction: any = {
        type: EIP712_TX_TYPE,
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

    if (
        (utils.hexlify(ethSignature.r) == '0x' || utils.hexlify(ethSignature.s) == '0x') &&
        !transaction.customData.customSignature
    ) {
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

function getSignature(transaction: any, ethSignature?: EthereumSignature): Uint8Array {
    if (transaction?.customData?.customSignature && transaction.customData.customSignature.length) {
        return ethers.utils.arrayify(transaction.customData.customSignature);
    }

    if (!ethSignature) {
        throw new Error('No signature provided');
    }

    const r = ethers.utils.zeroPad(ethers.utils.arrayify(ethSignature.r), 32);
    const s = ethers.utils.zeroPad(ethers.utils.arrayify(ethSignature.s), 32);
    const v = ethSignature.v;

    return new Uint8Array([...r, ...s, v]);
}

function eip712TxHash(transaction: any, ethSignature?: EthereumSignature) {
    const signedDigest = EIP712Signer.getSignedDigest(transaction);
    const hashedSignature = ethers.utils.keccak256(getSignature(transaction, ethSignature));

    return ethers.utils.keccak256(ethers.utils.hexConcat([signedDigest, hashedSignature]));
}

export function getL2HashFromPriorityOp(
    txReceipt: ethers.providers.TransactionReceipt,
    zkSyncAddress: Address
): string {
    let txHash: string = null;
    for (const log of txReceipt.logs) {
        if (log.address.toLowerCase() != zkSyncAddress.toLowerCase()) {
            continue;
        }

        try {
            const priorityQueueLog = ZKSYNC_MAIN_ABI.parseLog(log);
            if (priorityQueueLog && priorityQueueLog.args.txHash != null) {
                txHash = priorityQueueLog.args.txHash;
            }
        } catch {}
    }
    if (!txHash) {
        throw new Error('Failed to parse tx logs');
    }

    return txHash;
}

const ADDRESS_MODULO = BigNumber.from(2).pow(160);

export function applyL1ToL2Alias(address: string): string {
    return ethers.utils.hexlify(ethers.BigNumber.from(address).add(L1_TO_L2_ALIAS_OFFSET).mod(ADDRESS_MODULO));
}

export function undoL1ToL2Alias(address: string): string {
    let result = ethers.BigNumber.from(address).sub(L1_TO_L2_ALIAS_OFFSET);
    if (result.lt(BigNumber.from(0))) {
        result = result.add(ADDRESS_MODULO);
    }

    return ethers.utils.hexlify(result);
}

/// Getters data used to correctly initialize the L1 token counterpart on L2
async function getERC20GettersData(l1TokenAddress: string, provider: ethers.providers.Provider): Promise<string> {
    const token = IERC20MetadataFactory.connect(l1TokenAddress, provider);

    const name = await token.name();
    const symbol = await token.symbol();
    const decimals = await token.decimals();

    const coder = new AbiCoder();

    const nameBytes = coder.encode(['string'], [name]);
    const symbolBytes = coder.encode(['string'], [symbol]);
    const decimalsBytes = coder.encode(['uint256'], [decimals]);

    return coder.encode(['bytes', 'bytes', 'bytes'], [nameBytes, symbolBytes, decimalsBytes]);
}

/// The method that returns the calldata that will be sent by an L1 ERC20 bridge to its L2 counterpart
/// during bridging of a token.
export async function getERC20BridgeCalldata(
    l1TokenAddress: string,
    l1Sender: string,
    l2Receiver: string,
    amount: BigNumberish,
    provider: ethers.providers.Provider
): Promise<string> {
    const gettersData = await getERC20GettersData(l1TokenAddress, provider);
    return L2_BRIDGE_ABI.encodeFunctionData('finalizeDeposit', [
        l1Sender,
        l2Receiver,
        l1TokenAddress,
        amount,
        gettersData
    ]);
}

// The method with similar functionality is already available in ethers.js,
// the only difference is that we provide additional `try { } catch { }`
// for error-resilience.
//
// It will also pave the road for allowing future EIP-1271 signature verification, by
// letting our SDK have functionality to verify signatures.
function isECDSASignatureCorrect(address: string, msgHash: string, signature: SignatureLike): boolean {
    try {
        return address == ethers.utils.recoverAddress(msgHash, signature);
    } catch {
        // In case ECDSA signature verification has thrown an error,
        // we simply consider the signature as incorrect.
        return false;
    }
}

async function isEIP1271SignatureCorrect(
    provider: Provider,
    address: string,
    msgHash: string,
    signature: SignatureLike
): Promise<boolean> {
    const accountContract = new ethers.Contract(address, IERC1271, provider);

    // This line may throw an exception if the contract does not implement the EIP1271 correctly.
    // But it may also throw an exception in case the internet connection is lost.
    // It is the caller's responsibility to handle the exception.
    const result = await accountContract.isValidSignature(msgHash, signature);

    return result == EIP1271_MAGIC_VALUE;
}

async function isSignatureCorrect(
    provider: Provider,
    address: string,
    msgHash: string,
    signature: SignatureLike
): Promise<boolean> {
    let isContractAccount = false;

    const code = await provider.getCode(address);
    isContractAccount = ethers.utils.arrayify(code).length != 0;

    if (!isContractAccount) {
        return isECDSASignatureCorrect(address, msgHash, signature);
    } else {
        return await isEIP1271SignatureCorrect(provider, address, msgHash, signature);
    }
}

// Returns `true` or `false` depending on whether or not the account abstraction's
// signature is correct. Note, that while currently it does not do any `async` actions.
// in the future it will. That's why the `Promise<boolean>` is returned.
export async function isMessageSignatureCorrect(
    provider: Provider,
    address: string,
    message: ethers.Bytes | string,
    signature: SignatureLike
): Promise<boolean> {
    const msgHash = ethers.utils.hashMessage(message);
    return await isSignatureCorrect(provider, address, msgHash, signature);
}

// Returns `true` or `false` depending on whether or not the account abstraction's
// EIP712 signature is correct. Note, that while currently it does not do any `async` actions.
// in the future it will. That's why the `Promise<boolean>` is returned.
export async function isTypedDataSignatureCorrect(
    provider: Provider,
    address: string,
    domain: TypedDataDomain,
    types: Record<string, Array<TypedDataField>>,
    value: Record<string, any>,
    signature: SignatureLike
): Promise<boolean> {
    const msgHash = ethers.utils._TypedDataEncoder.hash(domain, types, value);
    return await isSignatureCorrect(provider, address, msgHash, signature);
}

export async function estimateDefaultBridgeDepositL2Gas(
    providerL1: ethers.providers.Provider,
    providerL2: Provider,
    token: Address,
    amount: BigNumberish,
    to: Address,
    from?: Address,
    gasPerPubdataByte?: BigNumberish
): Promise<BigNumber> {
    // If the `from` address is not provided, we use a random address, because
    // due to storage slot aggregation, the gas estimation will depend on the address
    // and so estimation for the zero address may be smaller than for the sender.
    from ??= ethers.Wallet.createRandom().address;

    if (token == ETH_ADDRESS) {
        return await providerL2.estimateL1ToL2Execute({
            contractAddress: to,
            gasPerPubdataByte: gasPerPubdataByte,
            caller: from,
            calldata: '0x',
            l2Value: amount
        });
    } else {
        const l1ERC20BridgeAddresses = (await providerL2.getDefaultBridgeAddresses()).erc20L1;
        const erc20BridgeAddress = (await providerL2.getDefaultBridgeAddresses()).erc20L2;

        const calldata = await getERC20BridgeCalldata(token, from, to, amount, providerL1);

        return await providerL2.estimateL1ToL2Execute({
            caller: applyL1ToL2Alias(l1ERC20BridgeAddresses),
            contractAddress: erc20BridgeAddress,
            gasPerPubdataByte: gasPerPubdataByte,
            calldata: calldata
        });
    }
}

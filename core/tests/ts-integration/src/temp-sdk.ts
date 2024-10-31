import * as zksync from 'zksync-ethers-interop-support';
import * as ethers from 'ethers';
import { BytesLike } from 'ethers';
import {
    L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR,
    BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI,
    INTEROP_BUNDLE_ABI,
    INTEROP_TRIGGER_ABI
} from './constants';

const L1_MESSENGER_ADDRESS = L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR;

export interface Output {
    output: any;
    rawData: any;
    l1BatchNumber: number;
    l2TxNumberInBlock: number;
}

export async function getInteropBundleData(
    provider: zksync.Provider,
    withdrawalHash: BytesLike,
    index = 0
): Promise<Output> {
    const response = await tryGetMessageData(provider, withdrawalHash, index);
    if (!response) return { rawData: null, output: null, l1BatchNumber: 0, l2TxNumberInBlock: 0 };
    const { message } = response!;

    // Decode the interop message
    const decodedRequest = ethers.AbiCoder.defaultAbiCoder().decode([INTEROP_BUNDLE_ABI], '0x' + message.slice(2));
    let calls = [];
    for (let i = 0; i < decodedRequest[0][1].length; i++) {
        calls.push({
            to: decodedRequest[0][1][i][0],
            from: decodedRequest[0][1][i][1],
            value: decodedRequest[0][1][i][2],
            data: decodedRequest[0][1][i][3]
        });
    }

    let executionAddresses = [];
    for (let i = 0; i < decodedRequest[0][2].length; i++) {
        executionAddresses.push(decodedRequest[0][2][i]);
    }

    const xl2Input = {
        destinationChainId: decodedRequest[0][0],
        calls: calls,
        executionAddresses: executionAddresses,
        cancellationAddress: decodedRequest[0][3]
    };
    let output: Output = {
        rawData: ethers.AbiCoder.defaultAbiCoder().encode([INTEROP_BUNDLE_ABI], [xl2Input]),
        output: xl2Input,
        l1BatchNumber: response.l1BatchNumber,
        l2TxNumberInBlock: response.l2TxNumberInBlock
    };
    return output;
}

export async function getInteropTriggerData(
    provider: zksync.Provider,
    withdrawalHash: BytesLike,
    index = 0
): Promise<Output> {
    // console.log("index", index)
    const response = await tryGetMessageData(provider, withdrawalHash, index);
    if (!response) return { rawData: null, output: null, l1BatchNumber: 0, l2TxNumberInBlock: 0 };
    const { message } = response!;

    // Decode the interop message
    // console.log("trigger message", message)
    // console.log("withdrawalHash", withdrawalHash)

    let decodedRequest = ethers.AbiCoder.defaultAbiCoder().decode([INTEROP_TRIGGER_ABI], '0x' + message.slice(2));

    // console.log("decodedRequest", decodedRequest)

    let trigger = false;
    if (decodedRequest[0][4]) {
        if (decodedRequest[0][4][1] == 800n) {
            trigger = true;
        }
    }
    if (!trigger) {
        throw new Error('Trigger is not found');
    }

    // let decodedCallRequest = ethers.AbiCoder.defaultAbiCoder().decode(
    //   [INTEROP_BUNDLE_ABI],
    //   '0x' + message.slice(2)
    // )
    // console.log("trigger", trigger)
    // console.log("decodedCallRequest", decodedRequest)
    // console.log("decodedCallRequest[0][0]", decodedRequest[0][2])
    let output = {
        destinationChainId: decodedRequest[0][0],
        from: decodedRequest[0][1],
        feeBundleHash: decodedRequest[0][2],
        executionBundleHash: decodedRequest[0][3],
        gasFields: {
            gasLimit: decodedRequest[0][4][0],
            gasPerPubdataByteLimit: decodedRequest[0][4][1],
            refundRecipient: decodedRequest[0][4][2]
        }
    };
    // console.log("output", output)

    return {
        rawData: ethers.AbiCoder.defaultAbiCoder().encode([INTEROP_TRIGGER_ABI], [output]),
        output: output,
        l1BatchNumber: response.l1BatchNumber,
        l2TxNumberInBlock: response.l2TxNumberInBlock
    };
}

//   export async function getL2CanonicalTransactionData(
//     provider: zksync.Provider,
//     withdrawalHash: BytesLike,
//     index = 0
//   ){
//     const response  = await tryGetMessageData(provider, withdrawalHash, index);
//     if (!response) return;
//     const { message } = response!;

//     // Decode the interop message
//     const decodedRequest = ethers.AbiCoder.defaultAbiCoder().decode(
//         [BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI],
//         '0x' + message.slice(2)
//     );

//     const xl2Input = {
//         txType: decodedRequest[0][0],
//         from: decodedRequest[0][1],
//         to: decodedRequest[0][2],
//         gasLimit: decodedRequest[0][3],
//         gasPerPubdataByteLimit: decodedRequest[0][4],
//         maxFeePerGas: decodedRequest[0][5],
//         maxPriorityFeePerGas: decodedRequest[0][6],
//         paymaster: decodedRequest[0][7],
//         nonce: decodedRequest[0][8],
//         value: decodedRequest[0][9],
//         reserved: [
//             decodedRequest[0][10][0],
//             decodedRequest[0][10][1],
//             decodedRequest[0][10][2],
//             decodedRequest[0][10][3]
//         ],
//         data: decodedRequest[0][11],
//         signature: decodedRequest[0][12],
//         factoryDeps: decodedRequest[0][13],
//         paymasterInput: decodedRequest[0][14],
//         reservedDynamic: decodedRequest[0][15]
//     };
//     return { output: xl2Input, l1BatchNumber: response.l1BatchNumber, l2TxNumberInBlock: response.l2TxNumberInBlock };
//   }

async function tryGetMessageData(provider: zksync.Provider, withdrawalHash: BytesLike, index = 0) {
    let { l1BatchNumber, l2TxNumberInBlock, message } = { l1BatchNumber: 0, l2TxNumberInBlock: 0, message: '' };

    try {
        // console.log("Reading interop message");
        // `getFinalizeWithdrawalParamsWithoutProof` is only available for wallet instance but not provider
        const sender_chain_utilityWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, provider);
        const {
            l1BatchNumber: l1BatchNumberRead,
            l2TxNumberInBlock: l2TxNumberInBlockRead,
            message: messageRead
        } = await getFinalizeWithdrawalParamsWithoutProof(provider, withdrawalHash, index);
        // console.log("Finished reading interop message");

        l1BatchNumber = l1BatchNumberRead || 0;
        l2TxNumberInBlock = l2TxNumberInBlockRead || 0;
        message = messageRead || '';

        if (!message) return;
    } catch (e) {
        console.log('Error reading interop message:', e); // note no error here, since we run out of txs sometime
        return;
    }
    return { l1BatchNumber, l2TxNumberInBlock, message };
}

async function getFinalizeWithdrawalParamsWithoutProof(
    provider: zksync.Provider,
    withdrawalHash: BytesLike,
    index = 0
): Promise<FinalizeWithdrawalParamsWithoutProof> {
    const { log, l1BatchTxId } = await getWithdrawalLog(provider, withdrawalHash, index);
    // const {l2ToL1LogIndex} = await this._getWithdrawalL2ToL1Log(
    //   withdrawalHash,
    //   index
    // );
    const sender = ethers.dataSlice(log.topics[1], 12);

    const message = ethers.AbiCoder.defaultAbiCoder().decode(['bytes'], log.data)[0];
    return {
        l1BatchNumber: log.l1BatchNumber,
        l2TxNumberInBlock: l1BatchTxId,
        message,
        sender
    };
}

async function getWithdrawalLog(provider: zksync.Provider, withdrawalHash: BytesLike, index = 0) {
    const hash = ethers.hexlify(withdrawalHash);
    const receipt = await provider.getTransactionReceipt(hash);
    if (!receipt) {
        throw new Error('Transaction is not mined!');
    }
    const log = receipt.logs.filter(
        (log) =>
            zksync.utils.isAddressEq(log.address, L1_MESSENGER_ADDRESS) &&
            log.topics[0] === ethers.id('L1MessageSent(address,bytes32,bytes)')
    )[index];

    return {
        log,
        l1BatchTxId: receipt.l1BatchTxIndex
    };
}

export interface FinalizeWithdrawalParamsWithoutProof {
    /** The L2 batch number where the withdrawal was processed. */
    l1BatchNumber: number | null;
    // /** The position in the L2 logs Merkle tree of the l2Log that was sent with the message. */
    // l2MessageIndex: number;
    /** The L2 transaction number in the batch, in which the log was sent. */
    l2TxNumberInBlock: number | null;
    /** The L2 withdraw data, stored in an L2 -> L1 message. */
    message: any;
    /** The L2 address which sent the log. */
    sender: string;
    //     /** The Merkle proof of the inclusion L2 -> L1 message about withdrawal initialization. */
    //     proof: string[];
}

// // Construct log for Merkle proof
// const log = {
//     l2ShardId: 0,
//     isService: true,
//     txNumberInBatch: l2TxNumberInBlock,
//     sender: L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR,
//     key: ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode(['address'], [interop1_wallet.address])),
//     value: ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode(['bytes'], [message]))
// };

// const leafHash = ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode([L2_LOG_STRING], [log]));

// const proof1 =
//     ethers.ZeroHash +
//     ethers.AbiCoder.defaultAbiCoder()
//         .encode(
//             ['uint256', 'uint256', 'uint256', 'bytes32'],
//             [(await sender_chain_provider.getNetwork()).chainId, l1BatchNumber, l2TxNumberInBlock, leafHash]
//         )
//         .slice(2);

// const interopTxAsCanonicalTx = {
//     txType: 253n,
//     from: interopTx.from,
//     to: interopTx.to,
//     gasLimit: interopTx.gasLimit,
//     gasPerPubdataByteLimit: 50000n,
//     maxFeePerGas: interopTx.maxFeePerGas,
//     maxPriorityFeePerGas: 0,
//     paymaster: interopTx.customData.paymaster_params.paymaster,
//     nonce: interopTx.nonce,
//     value: interopTx.value,
//     reserved: [interopTx.customData.toMint, interopTx.customData.refundRecipient, '0x00', '0x00'],
//     data: interopTx.data,
//     signature: '0x',
//     factoryDeps: [],
//     paymasterInput: '0x',
//     reservedDynamic: proof1
// };
// const encodedTx = ethers.AbiCoder.defaultAbiCoder().encode(
//     [BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI],
//     [interopTxAsCanonicalTx]
// );
// const interopTxHash = ethers.keccak256(ethers.getBytes(encodedTx));
// console.log('interopTxHash', interopTxHash);

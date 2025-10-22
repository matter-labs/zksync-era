import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { BytesLike } from 'ethers';
import { INTEROP_BUNDLE_ABI, MESSAGE_INCLUSION_PROOF_ABI, L2_INTEROP_CENTER_ADDRESS } from './constants';
// import { FinalizeWithdrawalParams } from 'zksync-ethers/build/types';

export interface Output {
    output: any;
    rawData: any;
    l1BatchNumber: number;
    l2TxNumberInBlock: number;
    l2MessageIndex: number;
    fullProof: string;
    proofDecoded: any;
}

export async function getInteropBundleData(
    provider: zksync.Provider,
    withdrawalHash: BytesLike,
    index = 0
): Promise<Output> {
    const response = await tryGetMessageData(provider, withdrawalHash, index);
    if (!response)
        return {
            rawData: null,
            output: null,
            l1BatchNumber: 0,
            l2TxNumberInBlock: 0,
            l2MessageIndex: 0,
            fullProof: '',
            proofDecoded: null
        };
    const { message } = response!;

    // Decode the interop message
    // console.log("message", message)
    const decodedRequest = ethers.AbiCoder.defaultAbiCoder().decode([INTEROP_BUNDLE_ABI], '0x' + message.slice(4));

    let calls = [];
    for (let i = 0; i < decodedRequest[0][4].length; i++) {
        calls.push({
            version: decodedRequest[0][4][i][0],
            shadowAccount: decodedRequest[0][4][i][1],
            to: decodedRequest[0][4][i][2],
            from: decodedRequest[0][4][i][3],
            value: decodedRequest[0][4][i][4],
            data: decodedRequest[0][4][i][5]
        });
    }
    // console.log(decodedRequest);

    const xl2Input = {
        version: decodedRequest[0][0],
        sourceChainId: decodedRequest[0][1],
        destinationChainId: decodedRequest[0][2],
        interopBundleSalt: decodedRequest[0][3],
        calls: calls,
        bundleAttributes: {
            executionAddress: decodedRequest[0][5][0],
            unbundlerAddress: decodedRequest[0][5][1]
        }
    };
    // console.log("response.proof", proof_fee)
    const rawData = ethers.AbiCoder.defaultAbiCoder().encode([INTEROP_BUNDLE_ABI], [xl2Input]);
    let proofEncoded = ethers.AbiCoder.defaultAbiCoder().encode(
        [MESSAGE_INCLUSION_PROOF_ABI],
        [
            {
                chainId: (await provider.getNetwork()).chainId,
                l1BatchNumber: response.l1BatchNumber,
                l2MessageIndex: response.l2MessageIndex,
                message: [response.l2TxNumberInBlock, L2_INTEROP_CENTER_ADDRESS, rawData],
                proof: response.proof
            }
        ]
    );
    let output: Output = {
        rawData: rawData,
        output: xl2Input,
        l1BatchNumber: response.l1BatchNumber,
        l2TxNumberInBlock: response.l2TxNumberInBlock,
        l2MessageIndex: response.l2MessageIndex,
        fullProof: proofEncoded,
        proofDecoded: {
            chainId: (await provider.getNetwork()).chainId,
            l1BatchNumber: response.l1BatchNumber,
            l2MessageIndex: response.l2MessageIndex,
            message: [response.l2TxNumberInBlock, L2_INTEROP_CENTER_ADDRESS, rawData],
            proof: response.proof
        }
    };
    return output;
}

async function tryGetMessageData(provider: zksync.Provider, withdrawalHash: BytesLike, index = 0) {
    let { l1BatchNumber, l2TxNumberInBlock, message, l2MessageIndex, proof } = {
        l1BatchNumber: 0,
        l2TxNumberInBlock: 0,
        message: '',
        l2MessageIndex: 0,
        proof: ['']
    };

    try {
        // console.log("Reading interop message");
        // `getFinalizeWithdrawalParamsWithoutProof` is only available for wallet instance but not provider
        const sender_chain_utilityWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, provider);
        // const { l2ToL1LogIndex, l2ToL1Log } = await sender_chain_utilityWallet._getWithdrawalL2ToL1Log(
        //     withdrawalHash,
        //     index
        // );
        // const gatewayChainId = 506;
        const {
            l1BatchNumber: l1BatchNumberRead,
            l2TxNumberInBlock: l2TxNumberInBlockRead,
            message: messageRead,
            l2MessageIndex: l2MessageIndexRead,
            proof: proofRead
        } = await sender_chain_utilityWallet.getFinalizeWithdrawalParams(withdrawalHash, index, 'proof_based_gw');
        // const logProof = await sender_chain_utilityWallet.provider.getLogProof(
        //     withdrawalHash,
        //     index,
        //     0,
        //     gatewayChainId
        // );
        // console.log({
        //     l2ToL1Log: l2ToL1Log,
        //     l2ToL1LogIndex: l2ToL1LogIndex,
        //     l1BatchNumberRead: l1BatchNumberRead,
        //     l2TxNumberInBlockRead: l2TxNumberInBlockRead,
        //     l2MessageIndexRead: l2MessageIndexRead,
        //     // "proofRead": proofRead,
        //     logProof: logProof
        // });

        // } = await getFinalizeWithdrawalParamsWithoutProof(provider, withdrawalHash, index);
        // console.log("Finished reading interop message");

        l1BatchNumber = l1BatchNumberRead || 0;
        l2TxNumberInBlock = l2TxNumberInBlockRead || 0;
        message = messageRead || '';
        l2MessageIndex = l2MessageIndexRead || 0;
        proof = proofRead || [''];

        if (!message) return;
    } catch (e) {
        console.log('Error reading interop message:', e); // note no error here, since we run out of txs sometime
        return;
    }
    return { l1BatchNumber, l2TxNumberInBlock, message, l2MessageIndex, proof };
}

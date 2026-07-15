import { TestMessage } from './matcher-helpers';
import { MatcherModifier } from '../modifiers';
import * as zksync from 'zksync-ethers';
import { AugmentedTransactionResponse } from '../transaction-response';
import { ethers } from 'ethers';

// This file contains implementation of matchers for ZKsync/ethereum transaction.
// For actual doc-comments, see `typings/jest.d.ts` file.

export async function toBeAccepted(
    txPromise: Promise<AugmentedTransactionResponse>,
    modifiers: MatcherModifier[] = [],
    additionalInfo?: string
) {
    try {
        const tx = await txPromise;
        const reporter = tx.reporter;
        reporter?.debug(`Waiting for transaction ${tx.hash} (from=${tx.from}, nonce=${tx.nonce}) to get accepted`);
        const receipt = <zksync.types.TransactionReceipt>await tx.wait();

        // Check receipt validity.
        const failReason = checkReceiptFields(tx, receipt);
        if (failReason) {
            return failReason;
        }

        // Apply modifiers.
        for (const modifier of modifiers) {
            const failReason = await modifier.check(receipt);
            if (failReason) {
                return failReason;
            }
        }

        reporter?.debug(`Transaction ${tx.hash} is accepted as expected`);
        return pass();
    } catch (error: any) {
        // Check if an error was raised by `jest` (e.g. by using `expect` inside of the modifier).
        if (error.matcherResult) {
            // It was, just re-broadcast it.
            throw error;
        }

        const message = new TestMessage()
            .matcherHint('.toBeAccepted')
            .line('Transaction was expected to pass, but it failed. Details:')
            .received(error)
            .additional(additionalInfo)
            .build();
        return fail(message);
    }
}

export async function toBeReverted(
    txPromise: AugmentedTransactionResponse | Promise<AugmentedTransactionResponse>,
    modifiers: MatcherModifier[] = [],
    additionalInfo?: string
) {
    try {
        const tx = await txPromise;
        const reporter = tx.reporter;
        reporter?.debug(`Waiting for transaction ${tx.hash} (from=${tx.from}, nonce=${tx.nonce}) to get reverted`);
        const receipt = await tx.wait();

        const message = new TestMessage()
            .matcherHint('.toBeReverted')
            .line('Transaction was expected to be reverted, but it succeeded. Receipt:')
            .received(receipt)
            .additional(additionalInfo)
            .build();

        return fail(message);
    } catch (error: any) {
        // kl todo remove this once ethers is fixed
        if (error.toString().includes('reverted')) {
            return pass();
        }
        const receipt = error.receipt;
        if (!receipt) {
            const message = new TestMessage()
                .matcherHint('.toBeReverted')
                .line('Received malformed error message (without transaction receipt)')
                .received(error)
                .build();
            return fail(message);
        }

        for (const modifier of modifiers) {
            const failReason = await modifier.check(receipt);
            if (failReason) {
                return failReason;
            }
        }

        return pass();
    }
}

export async function toBeRevertedEthCall(
    txPromise: Promise<any>,
    revertReason?: string,
    encodedRevertReason?: string,
    additionalInfo?: string
) {
    return await toBeRejectedWithPrefix(
        txPromise,
        'call revert exception; VM Exception while processing transaction: reverted with reason string "',
        revertReason,
        encodedRevertReason,
        additionalInfo
    );
}

export async function toBeRevertedEstimateGas(
    txPromise: Promise<any>,
    revertReason?: string,
    encodedRevertReason?: string,
    additionalInfo?: string
) {
    return await toBeRejectedWithPrefix(
        txPromise,
        'execution reverted: ',
        revertReason,
        encodedRevertReason,
        additionalInfo
    );
}

async function toBeRejectedWithPrefix(
    txPromise: Promise<any>,
    prefix: string,
    errorSubstring?: string,
    dataSubstring?: string,
    additionalInfo?: string
) {
    try {
        const tx = await txPromise;
        // Unlike with `toBeReverted` test, we don't even need to wait for the transaction to be executed.
        // We expect it to be rejected by the API server.

        const message = new TestMessage()
            .matcherHint('.toBeRejected')
            .line('Transaction was expected to be rejected by the API server, but it was not. Receipt:')
            .received(tx)
            .additional(additionalInfo)
            .build();

        return fail(message);
    } catch (error: any) {
        if (errorSubstring) {
            // We expect thrown exception to always have the `message` field.
            let fullErrorSubstring = `${prefix}${errorSubstring}`;
            if (!error.message || !error.message.includes(fullErrorSubstring)) {
                const message = new TestMessage()
                    .matcherHint('.toBeRejected')
                    .line('Transaction was expected to be rejected by the API server with the following message:')
                    .expected(errorSubstring)
                    .line("but it wasn't detected. Received error:")
                    .received(error)
                    .additional(additionalInfo)
                    .build();

                return fail(message);
            }
        }

        if (dataSubstring) {
            // We expect thrown exception to always have the `data` field.
            if (!error.message || !error.message.includes(dataSubstring)) {
                const message = new TestMessage()
                    .matcherHint('.toBeRejected')
                    .line('Transaction was expected to be rejected by the API server with the following data:')
                    .expected(dataSubstring)
                    .line("but it wasn't detected. Received error:")
                    .received(error)
                    .additional(additionalInfo)
                    .build();

                return fail(message);
            }
        }
        return pass();
    }
}

export async function toBeRejected(txPromise: Promise<any>, errorSubstring?: string, additionalInfo?: string) {
    return await toBeRejectedWithPrefix(txPromise, '', errorSubstring, undefined, additionalInfo);
}

// Local helper to mark transaction test as passed.
function pass() {
    const message =
        "No details available. \
        Make sure that you don't use transaction matchers with a `.not` modifier, as it's not supported. \
        If you don't, probably there exists a bug in the framework";
    return {
        pass: true,
        message: () => message
    };
}

// Local helper to mark transaction test as failed.
function fail(message: string) {
    return {
        pass: false,
        message: () => message
    };
}

/**
 * Checks that the values in the receipt correspond to the values in the transaction request.
 *
 * @returns If check has failed, returns a Jest error object. Otherwise, returns `undefined`.
 */
function checkReceiptFields(request: ethers.TransactionResponseParams, receipt: zksync.types.TransactionReceipt) {
    const errorMessageBuilder = new TestMessage()
        .matcherHint('.checkReceiptFields')
        .line('Transaction receipt is not properly formatted. Transaction request:')
        .expected(request)
        .line('Transaction receipt:')
        .received(receipt);
    const failWith = (line: string) => fail(errorMessageBuilder.line(line).build());

    if (receipt.status !== 0 && receipt.status !== 1) {
        return failWith(`Status field in the receipt has an unexpected value (expected 0 or 1): ${receipt.status}`);
    }
    const effectiveGasPrice = receipt.gasUsed * receipt.gasPrice;
    if (effectiveGasPrice <= 0n) {
        return failWith(`Effective gas price expected to be greater than 0`);
    }
    if (!receipt.gasUsed) {
        return failWith(`Gas used expected to be greater than 0`);
    }
    return undefined;
}

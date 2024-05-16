import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { Reporter } from './reporter';
import { TransactionResponse } from 'zksync-ethers/build/types';

/**
 * RetryProvider retries every RPC request if it detects a timeout-related issue on the server side.
 */
export class RetryProvider extends zksync.Provider {
    private readonly reporter: Reporter;

    constructor(
        url?: string | ethers.ethers.utils.ConnectionInfo | undefined,
        network?: ethers.ethers.providers.Networkish | undefined,
        reporter?: Reporter
    ) {
        super(url, network);
        this.reporter = reporter ?? new Reporter();
    }

    override async send(method: string, params: any): Promise<any> {
        for (let retry = 0; retry < 50; retry++) {
            try {
                const result = await super.send(method, params);
                // If we obtained result not from the first attempt, print a warning.
                if (retry != 0) {
                    this.reporter?.debug(`Request for method ${method} took ${retry} retries to succeed`);
                }
                return result;
            } catch (err: any) {
                // Error markers observed on stage so far.
                const ignoredErrors = [
                    'timeout',
                    'etimedout',
                    'econnrefused',
                    'econnreset',
                    'bad gateway',
                    'service temporarily unavailable',
                    'nonetwork'
                ];
                const errString: string = err.toString().toLowerCase();
                const found = ignoredErrors.some((sampleErr) => errString.indexOf(sampleErr) !== -1);
                if (found) {
                    // Error is related to timeouts. Sleep a bit and try again.
                    await zksync.utils.sleep(this.pollingInterval);
                    continue;
                }
                // Re-throw any non-timeout-related error.
                throw err;
            }
        }
    }

    override _wrapTransaction(tx: ethers.Transaction, hash?: string): AugmentedTransactionResponse {
        const wrapped = super._wrapTransaction(tx, hash);
        const originalWait = wrapped.wait;
        wrapped.wait = async (confirmations) => {
            this.reporter.debug(`Started waiting for transaction ${tx.hash} (from=${tx.from}, nonce=${tx.nonce})`);
            const receipt = await originalWait(confirmations);
            this.reporter.debug(
                `Obtained receipt for transaction ${tx.hash}: blockNumber=${receipt.blockNumber}, status=${receipt.status}`
            );
            return receipt;
        };
        return { ...wrapped, reporter: this.reporter };
    }
}

export interface AugmentedTransactionResponse extends TransactionResponse {
    readonly reporter?: Reporter;
}

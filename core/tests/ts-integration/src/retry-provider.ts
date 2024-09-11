import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { Reporter } from './reporter';
import { AugmentedTransactionResponse } from './transaction-response';

/**
 * RetryProvider retries every RPC request if it detects a timeout-related issue on the server side.
 */
export class RetryProvider extends zksync.Provider {
    private readonly reporter: Reporter;
    private readonly knownTransactionHashes: Set<string> = new Set();

    constructor(_url?: string | { url: string; timeout: number }, network?: ethers.Networkish, reporter?: Reporter) {
        let url;
        if (typeof _url === 'object') {
            const fetchRequest: ethers.FetchRequest = new ethers.FetchRequest(_url.url);
            fetchRequest.timeout = _url.timeout;
            url = fetchRequest;
        } else {
            url = _url;
        }

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

    override _wrapTransactionResponse(txResponse: any): L2TransactionResponse {
        const base = super._wrapTransactionResponse(txResponse);
        this.knownTransactionHashes.add(base.hash);
        return new L2TransactionResponse(base, this.reporter);
    }

    override _wrapTransactionReceipt(receipt: any): zksync.types.TransactionReceipt {
        const wrapped = super._wrapTransactionReceipt(receipt);
        if (!this.knownTransactionHashes.has(receipt.transactionHash)) {
            this.knownTransactionHashes.add(receipt.transactionHash);
            this.reporter.debug(
                `Obtained receipt for L2 transaction ${receipt.transactionHash}: blockNumber=${receipt.blockNumber}, status=${receipt.status}`
            );
        }
        return wrapped;
    }
}

class L2TransactionResponse extends zksync.types.TransactionResponse implements AugmentedTransactionResponse {
    public readonly kind = 'L2';

    constructor(base: zksync.types.TransactionResponse, public readonly reporter: Reporter) {
        super(base, base.provider);
    }

    override async wait(confirmations?: number) {
        this.reporter.debug(`Started waiting for L2 transaction ${this.hash} (from=${this.from}, nonce=${this.nonce})`);
        const receipt = await super.wait(confirmations);
        if (receipt !== null) {
            this.reporter.debug(
                `Obtained receipt for L2 transaction ${this.hash}: blockNumber=${receipt.blockNumber}, status=${receipt.status}`
            );
        }
        return receipt;
    }

    override replaceableTransaction(startBlock: number): L2TransactionResponse {
        const base = super.replaceableTransaction(startBlock);
        return new L2TransactionResponse(base, this.reporter);
    }
}

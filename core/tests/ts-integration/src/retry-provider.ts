import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { Reporter } from './reporter';
import { AugmentedTransactionResponse } from './transaction-response';
import { JsonRpcProvider, Network, TransactionRequest, TransactionResponse, TransactionResponseParams } from 'ethers';

// Error markers observed on stage so far.
const IGNORED_ERRORS = [
    'timeout',
    'etimedout',
    'econnrefused',
    'econnreset',
    'bad gateway',
    'service temporarily unavailable',
    'nonetwork'
];

function isIgnored(err: any): boolean {
    const errString: string = err.toString().toLowerCase();
    return IGNORED_ERRORS.some((sampleErr) => errString.indexOf(sampleErr) !== -1);
}

export type UrlLike = string | { url: string; timeout: number };

export function createFetchRequest(
    url: UrlLike,
    getPollingInterval: () => number,
    reporter?: Reporter
): ethers.FetchRequest {
    let fetchRequest: ethers.FetchRequest;
    if (typeof url === 'object') {
        fetchRequest = new ethers.FetchRequest(url.url);
        fetchRequest.timeout = url.timeout;
    } else {
        fetchRequest = new ethers.FetchRequest(url);
    }
    const defaultGetUrlFunc = ethers.FetchRequest.createGetUrlFunc();
    fetchRequest.getUrlFunc = async (req: ethers.FetchRequest, signal?: ethers.FetchCancelSignal) => {
        // Retry network requests that failed because of temporary issues (such as timeout, econnreset).
        for (let retry = 0; retry < 50; retry++) {
            try {
                const result = await defaultGetUrlFunc(req, signal);
                // If we obtained result not from the first attempt, print a warning.
                if (retry != 0) {
                    reporter?.debug(`RPC request ${req} took ${retry} retries to succeed`);
                }
                return result;
            } catch (err: any) {
                if (isIgnored(err)) {
                    // Error is related to timeouts. Sleep a bit and try again.
                    await zksync.utils.sleep(getPollingInterval());
                    continue;
                }
                // Re-throw any non-timeout-related error.
                throw err;
            }
        }
        return Promise.reject(new Error(`Retried too many times, giving up on request=${req}`));
    };
    return fetchRequest;
}

/**
 * RetryProvider retries every RPC request if it detects a timeout-related issue on the server side.
 */
export class RetryProvider extends zksync.Provider {
    public readonly reporter: Reporter;
    private readonly knownTransactionHashes: Set<string> = new Set();

    constructor(url: UrlLike, network?: ethers.Networkish, reporter?: Reporter) {
        const fetchRequest = createFetchRequest(url, () => this.pollingInterval, reporter);
        super(fetchRequest, network);
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
                if (isIgnored(err)) {
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
        const isNew = !this.knownTransactionHashes.has(base.hash);
        if (isNew) {
            this.reporter.debug(
                `Created L2 transaction ${base.hash} (from=${base.from}, to=${base.to}, ` +
                    `nonce=${base.nonce}, value=${base.value}, data=${debugData(base.data)})`
            );
        }

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
    private isWaitingReported: boolean = false;
    private isReceiptReported: boolean = false;

    constructor(
        base: zksync.types.TransactionResponse,
        public readonly reporter: Reporter
    ) {
        super(base, base.provider);
    }

    override async wait(confirmations?: number) {
        if (!this.isWaitingReported) {
            this.reporter.debug(
                `Started waiting for L2 transaction ${this.hash} (from=${this.from}, nonce=${this.nonce})`
            );
            this.isWaitingReported = true;
        }
        const receipt = await super.wait(confirmations);
        if (receipt !== null && !this.isReceiptReported) {
            this.reporter.debug(
                `Obtained receipt for L2 transaction ${this.hash}: blockNumber=${receipt.blockNumber}, status=${receipt.status}`
            );
            this.isReceiptReported = true;
        }
        return receipt;
    }

    override replaceableTransaction(startBlock: number): L2TransactionResponse {
        const base = super.replaceableTransaction(startBlock);
        return new L2TransactionResponse(base, this.reporter);
    }
}

/** Retriable provider based on `ethers.JsonRpcProvider` */
export class EthersRetryProvider extends JsonRpcProvider {
    readonly reporter: Reporter;

    constructor(
        url: UrlLike,
        private readonly layer: 'L1' | 'L2',
        reporter?: Reporter
    ) {
        const fetchRequest = createFetchRequest(url, () => this.pollingInterval, reporter);
        super(fetchRequest, undefined, { batchMaxCount: 1 });
        this.reporter = reporter ?? new Reporter();
    }

    override _wrapTransactionResponse(tx: TransactionResponseParams, network: Network): EthersTransactionResponse {
        const base = super._wrapTransactionResponse(tx, network);
        return new EthersTransactionResponse(base, this.layer, this.reporter);
    }
}

class EthersTransactionResponse extends ethers.TransactionResponse implements AugmentedTransactionResponse {
    private isWaitingReported: boolean = false;
    private isReceiptReported: boolean = false;

    constructor(
        base: ethers.TransactionResponse,
        public readonly kind: 'L1' | 'L2',
        public readonly reporter: Reporter
    ) {
        super(base, base.provider);
    }

    override async wait(confirmations?: number, timeout?: number) {
        if (!this.isWaitingReported) {
            this.reporter.debug(
                `Started waiting for ${this.kind} transaction ${this.hash} (from=${this.from}, nonce=${this.nonce}, gasLimit=${this.gasLimit})`
            );
            this.isWaitingReported = true;
        }

        const receipt = await super.wait(confirmations, timeout);
        if (receipt !== null && !this.isReceiptReported) {
            this.reporter.debug(
                `Obtained receipt for ${this.kind} transaction ${this.hash}: blockNumber=${receipt.blockNumber}, status=${receipt.status}`
            );
            this.isReceiptReported = true;
        }
        return receipt;
    }

    override replaceableTransaction(startBlock: number): EthersTransactionResponse {
        const base = super.replaceableTransaction(startBlock);
        return new EthersTransactionResponse(base, this.kind, this.reporter);
    }
}

/** Wallet that retries `sendTransaction` requests on "nonce expired" errors, provided that it's possible (i.e., no nonce is set in the request). */
export class RetryableL1Wallet extends ethers.Wallet {
    constructor(key: string, provider: EthersRetryProvider) {
        super(key, provider);
    }

    override async sendTransaction(tx: TransactionRequest): Promise<TransactionResponse> {
        const reporter = (<EthersRetryProvider>this.provider!).reporter;
        while (true) {
            try {
                return await super.sendTransaction(tx);
            } catch (err: any) {
                // For unknown reason, `reth` sometimes returns outdated transaction count under load, leading to transactions getting rejected.
                // This is a workaround for this issue.
                reporter.debug('L1 transaction request failed', tx, err);
                if (err.code === 'NONCE_EXPIRED' && (tx.nonce === null || tx.nonce === undefined)) {
                    reporter.debug('Retrying L1 transaction request', tx);
                } else {
                    throw err;
                }
            }
        }
    }
}

/** Wallet that retries expired nonce errors for L1 transactions. */
export class RetryableWallet extends zksync.Wallet {
    constructor(privateKey: string, l2Provider: RetryProvider, l1Provider: EthersRetryProvider) {
        super(privateKey, l2Provider, l1Provider);
    }

    override ethWallet(): RetryableL1Wallet {
        return new RetryableL1Wallet(this.privateKey, <EthersRetryProvider>this._providerL1());
    }

    async retryableDepositCheck<R>(
        depositData: any,
        check: (deposit: zksync.types.PriorityOpResponse) => Promise<R>,
        maxAttempts: number = 3
    ): Promise<R> {
        const reporter = (<RetryProvider>this.provider!).reporter;
        let previousGasLimit: bigint | null = null;
        for (let i = 0; i < maxAttempts; i += 1) {
            let deposit: zksync.types.PriorityOpResponse | null = null;
            try {
                if (previousGasLimit) {
                    const newGasLimit: bigint = previousGasLimit * 2n;
                    if (depositData.overrides) {
                        depositData.overrides.gasLimit = newGasLimit;
                    } else {
                        depositData.overrides = { gasLimit: newGasLimit };
                    }
                    previousGasLimit = newGasLimit;
                }
                deposit = await this.deposit(depositData);
                if (!previousGasLimit) {
                    previousGasLimit = deposit.gasLimit;
                }
                return await check(deposit);
            } catch (err: any) {
                if (i + 1 == maxAttempts) {
                    reporter.debug('Last deposit check failed', deposit, err);
                    throw err;
                } else {
                    reporter.debug('Retrying deposit check', deposit, err);
                }
            }
        }
        throw 'unreachable';
    }
}

function debugData(rawData: string): string {
    const MAX_PRINTED_BYTE_LEN = 128;

    const byteLength = (rawData.length - 2) / 2;
    return byteLength <= MAX_PRINTED_BYTE_LEN
        ? rawData
        : rawData.substring(0, 2 + MAX_PRINTED_BYTE_LEN * 2) + `...(${byteLength} bytes)`;
}

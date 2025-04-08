import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { Reporter } from './reporter';
import { AugmentedTransactionResponse } from './transaction-response';
import { L1Provider, RetryableL1Wallet } from './l1-provider';

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

function createFetchRequest(
    url: string | { url: string; timeout: number },
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

async function _send(
    sender: (method: string, params: any) => Promise<any>,
    pollingInterval: number,
    reporter: Reporter,
    method: string,
    params: any
): Promise<any> {
    for (let retry = 0; retry < 50; retry++) {
        try {
            const result = await sender(method, params);
            // If we obtained result not from the first attempt, print a warning.
            if (retry != 0) {
                reporter.debug(`Request for method ${method} took ${retry} retries to succeed`);
            }
            return result;
        } catch (err: any) {
            if (isIgnored(err)) {
                // Error is related to timeouts. Sleep a bit and try again.
                await zksync.utils.sleep(pollingInterval);
                continue;
            }
            // Re-throw any non-timeout-related error.
            throw err;
        }
    }
}

function _handleTransactionResponse(
    knownTransactionHashes: Set<string>,
    reporter: Reporter,
    base: ethers.TransactionResponse
) {
    if (!knownTransactionHashes.has(base.hash)) {
        reporter.debug(
            `Created L2 transaction ${base.hash} (from=${base.from}, to=${base.to}, ` +
                `nonce=${base.nonce}, value=${base.value}, data=${debugData(base.data)})`
        );
    }
    knownTransactionHashes.add(base.hash);
}

function _handleTransactionReceipt(knownTransactionHashes: Set<string>, reporter: Reporter, receipt: any) {
    if (!knownTransactionHashes.has(receipt.transactionHash)) {
        knownTransactionHashes.add(receipt.transactionHash);
        reporter.debug(
            `Obtained receipt for L2 transaction ${receipt.transactionHash}: blockNumber=${receipt.blockNumber}, status=${receipt.status}`
        );
    }
}

/**
 * RetryProvider retries every RPC request if it detects a timeout-related issue on the server side.
 */
export class RetryProvider extends zksync.Provider {
    readonly reporter: Reporter;
    private readonly knownTransactionHashes: Set<string> = new Set();

    constructor(url: string | { url: string; timeout: number }, network?: ethers.Networkish, reporter?: Reporter) {
        const fetchRequest = createFetchRequest(url, () => this.pollingInterval, reporter);
        super(fetchRequest, network);
        this.reporter = reporter ?? new Reporter();
    }

    override async send(method: string, params: any): Promise<any> {
        return _send(
            (method, params) => super.send(method, params),
            this.pollingInterval,
            this.reporter,
            method,
            params
        );
    }

    override _wrapTransactionResponse(txResponse: any): L2TransactionResponse {
        const base = super._wrapTransactionResponse(txResponse);
        _handleTransactionResponse(this.knownTransactionHashes, this.reporter, base);
        return new L2TransactionResponse(base, this.reporter);
    }

    override _wrapTransactionReceipt(receipt: any): zksync.types.TransactionReceipt {
        _handleTransactionReceipt(this.knownTransactionHashes, this.reporter, receipt);
        return super._wrapTransactionReceipt(receipt);
    }
}

export class EthersRetryProvider extends ethers.JsonRpcProvider {
    readonly reporter: Reporter;
    private readonly knownTransactionHashes: Set<string> = new Set();

    constructor(url: string | { url: string; timeout: number }, network?: ethers.Networkish, reporter?: Reporter) {
        const fetchRequest = createFetchRequest(url, () => this.pollingInterval, reporter);
        super(fetchRequest, network);
        this.reporter = reporter ?? new Reporter();
    }

    override async send(method: string, params: any): Promise<any> {
        return _send(
            (method, params) => super.send(method, params),
            this.pollingInterval,
            this.reporter,
            method,
            params
        );
    }

    override _wrapTransactionResponse(txResponse: any, network: ethers.Network) {
        const base = super._wrapTransactionResponse(txResponse, network);
        _handleTransactionResponse(this.knownTransactionHashes, this.reporter, base);
        return new EthersTransactionResponse(base, this.reporter);
    }

    override _wrapTransactionReceipt(receipt: any, network: ethers.Network): ethers.TransactionReceipt {
        _handleTransactionReceipt(this.knownTransactionHashes, this.reporter, receipt);
        return super._wrapTransactionReceipt(receipt, network);
    }
}

interface ReportingTransactionResponse extends ethers.TransactionResponse {
    readonly reporter: Reporter;
    isWaitingReported: boolean;
    isReceiptReported: boolean;
}

async function _doWait<T extends ethers.TransactionReceipt>(
    base: ReportingTransactionResponse,
    receiptPromise: Promise<T | null>
): Promise<T | null> {
    if (!base.isWaitingReported) {
        base.reporter.debug(`Started waiting for L2 transaction ${base.hash} (from=${base.from}, nonce=${base.nonce})`);
        base.isWaitingReported = true;
    }
    const receipt = await receiptPromise;
    if (receipt !== null && !base.isReceiptReported) {
        base.reporter.debug(
            `Obtained receipt for L2 transaction ${base.hash}: blockNumber=${receipt.blockNumber}, status=${receipt.status}`
        );
        base.isReceiptReported = true;
    }
    return receipt;
}

class L2TransactionResponse
    extends zksync.types.TransactionResponse
    implements AugmentedTransactionResponse, ReportingTransactionResponse
{
    public readonly kind = 'L2';
    public isWaitingReported: boolean = false;
    public isReceiptReported: boolean = false;

    constructor(
        base: zksync.types.TransactionResponse,
        public readonly reporter: Reporter
    ) {
        super(base, base.provider);
    }

    override async wait(confirmations?: number): Promise<zksync.types.TransactionReceipt> {
        // The `zksync-ethers` library has incorrect typing for the return value; it can be `null`
        return (await _doWait(this, super.wait(confirmations))) as any;
    }

    override replaceableTransaction(startBlock: number): L2TransactionResponse {
        const base = super.replaceableTransaction(startBlock);
        return new L2TransactionResponse(base, this.reporter);
    }
}

class EthersTransactionResponse extends ethers.TransactionResponse implements ReportingTransactionResponse {
    public isWaitingReported: boolean = false;
    public isReceiptReported: boolean = false;

    constructor(
        base: ethers.TransactionResponse,
        public readonly reporter: Reporter
    ) {
        super(base, base.provider);
    }

    override async wait(confirmations?: number): Promise<ethers.TransactionReceipt | null> {
        return _doWait(this, super.wait(confirmations));
    }

    override replaceableTransaction(startBlock: number): EthersTransactionResponse {
        const base = super.replaceableTransaction(startBlock);
        return new EthersTransactionResponse(base, this.reporter);
    }
}

/** Wallet that retries expired nonce errors for L1 transactions. */
export class RetryableWallet extends zksync.Wallet {
    constructor(privateKey: string, l2Provider: RetryProvider, l1Provider: L1Provider) {
        super(privateKey, l2Provider, l1Provider);
    }

    override ethWallet(): RetryableL1Wallet {
        return new RetryableL1Wallet(this.privateKey, <L1Provider>this._providerL1());
    }
}

function debugData(rawData: string): string {
    const MAX_PRINTED_BYTE_LEN = 128;

    const byteLength = (rawData.length - 2) / 2;
    return byteLength <= MAX_PRINTED_BYTE_LEN
        ? rawData
        : rawData.substring(0, 2 + MAX_PRINTED_BYTE_LEN * 2) + `...(${byteLength} bytes)`;
}

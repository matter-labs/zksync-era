import {
    AddressLike,
    BlockTag,
    ethers,
    JsonRpcProvider,
    Network,
    TransactionRequest,
    TransactionResponse,
    TransactionResponseParams
} from 'ethers';
import { Reporter } from './reporter';
import { AugmentedTransactionResponse } from './transaction-response';

export class L1Provider extends JsonRpcProvider {
    readonly reporter: Reporter;

    constructor(url: string, reporter?: Reporter) {
        super(url, undefined, { batchMaxCount: 1 });
        this.reporter = reporter ?? new Reporter();
    }

    override async getTransactionCount(address: AddressLike, blockTag?: BlockTag): Promise<number> {
        const count = await super.getTransactionCount(address, blockTag);
        this.reporter.debug(`Received L1 transaction count for ${address} at ${blockTag}: ${count}`);
        return count;
    }

    override _wrapTransactionResponse(tx: TransactionResponseParams, network: Network): L1TransactionResponse {
        const base = super._wrapTransactionResponse(tx, network);
        return new L1TransactionResponse(base, this.reporter);
    }
}

class L1TransactionResponse extends ethers.TransactionResponse implements AugmentedTransactionResponse {
    public readonly kind = 'L1';
    private isWaitingReported: boolean = false;
    private isReceiptReported: boolean = false;

    constructor(base: ethers.TransactionResponse, public readonly reporter: Reporter) {
        super(base, base.provider);
    }

    override async wait(confirmations?: number, timeout?: number) {
        if (!this.isWaitingReported) {
            this.reporter.debug(
                `Started waiting for L1 transaction ${this.hash} (from=${this.from}, nonce=${this.nonce})`
            );
            this.isWaitingReported = true;
        }

        const receipt = await super.wait(confirmations, timeout);
        if (receipt !== null && !this.isReceiptReported) {
            this.reporter.debug(
                `Obtained receipt for L1 transaction ${this.hash}: blockNumber=${receipt.blockNumber}, status=${receipt.status}`
            );
            this.isReceiptReported = true;
        }
        return receipt;
    }

    override replaceableTransaction(startBlock: number): L1TransactionResponse {
        const base = super.replaceableTransaction(startBlock);
        return new L1TransactionResponse(base, this.reporter);
    }
}

/** Wallet that retries `sendTransaction` requests on "nonce expired" errors, provided that it's possible (i.e., no nonce is set in the request). */
export class RetryableL1Wallet extends ethers.Wallet {
    constructor(key: string, provider: L1Provider) {
        super(key, provider);
    }

    override async sendTransaction(tx: TransactionRequest): Promise<TransactionResponse> {
        const reporter = (<L1Provider>this.provider!).reporter;
        reporter.debug('Sending L1 transaction', tx);
        while (true) {
            try {
                return await super.sendTransaction(tx);
            } catch (err: any) {
                reporter.debug('L1 transaction request failed', tx, err);
                if (err.code === 'NONCE_EXPIRED' && tx.nonce === null) {
                    reporter.debug('Retrying L1 transaction request', tx);
                } else {
                    throw err;
                }
            }
        }
    }
}

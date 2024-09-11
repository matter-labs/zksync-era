import { ethers, JsonRpcProvider, Network, TransactionResponseParams } from 'ethers';
import { Reporter } from './reporter';
import { AugmentedTransactionResponse } from './transaction-response';

export class L1Provider extends JsonRpcProvider {
    private readonly reporter: Reporter;
    private readonly lastTransactionCount: Map<string, number> = new Map();

    constructor(url: string, reporter?: Reporter) {
        super(url, undefined, { batchMaxCount: 1 });
        this.reporter = reporter ?? new Reporter();
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

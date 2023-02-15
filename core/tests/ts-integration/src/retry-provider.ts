import * as zksync from 'zksync-web3';
import * as ethers from 'ethers';

/**
 * RetryProvider retries every RPC request if it detects a timeout-related issue on the server side.
 */
export class RetryProvider extends zksync.Provider {
    constructor(
        url?: string | ethers.ethers.utils.ConnectionInfo | undefined,
        network?: ethers.ethers.providers.Networkish | undefined
    ) {
        super(url, network);
    }

    override async send(method: string, params: any): Promise<any> {
        for (let retry = 0; retry < 50; retry++) {
            try {
                const result = await super.send(method, params);
                // If we obtained result not from the first attempt, print a warning.
                if (retry != 0) {
                    console.log(`Request for method ${method} took ${retry} retries to succeed`);
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
}

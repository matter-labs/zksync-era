/**
 * Base interface for custom transaction matcher modifiers.
 */

import * as zksync from 'zksync-ethers';

/**
 * Base class for custom transaction matcher modifiers.
 * Matcher can be applied to both a succeeded and reverted transaction
 * (but not a rejected one, since it's not even executed by the server).
 */
export abstract class MatcherModifier {
    /**
     * Asynchronous checker function.
     *
     * @param receipt Corresponding L2 transaction receipt.
     * @returns Should return `null` if check is passed and `MatcherMessage` otherwise.
     */
    abstract check(receipt: zksync.types.TransactionReceipt): Promise<MatcherMessage | null>;
}

/**
 * Object to be returned from a matcher modifier in case the check is failed.
 */
export interface MatcherMessage {
    pass: boolean;
    message: () => string;
}

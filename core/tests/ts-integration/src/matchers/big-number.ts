import { BigNumber, BigNumberish } from 'ethers';
import { TestMessage } from './matcher-helpers';

// Note: I attempted to "overload" the existing matchers from Jest (like `toBeGreaterThan`),
// but failed. There is a proposed hack in one GitHub issue from 2018: if you'll be trying to
// do the same, know: this hack doesn't work anymore. Default matchers rely on `this` to have
// certain properties, so attempt to load default matchers from `build` directory and call them
// as a fallback won't work (or I failed to make it work).

// This file contains implementation of matchers for BigNumber objects.
// For actual doc-comments, see `typings/jest.d.ts` file.

// Matcher for `l.gt(r)`
export function bnToBeGt(l: BigNumberish, r: BigNumberish, additionalInfo?: string) {
    const comparator = (l: BigNumber, r: BigNumber) => l.gt(r);
    const matcherName = `bnToBeGt`;
    const matcherMessage = `greater than`;
    return matcherBody(l, r, comparator, matcherName, matcherMessage, additionalInfo);
}

// Matcher for `l.gte(r)`
export function bnToBeGte(l: BigNumberish, r: BigNumberish, additionalInfo?: string) {
    const comparator = (l: BigNumber, r: BigNumber) => l.gte(r);
    const matcherName = `bnToBeGte`;
    const matcherMessage = `greater or equal than`;
    return matcherBody(l, r, comparator, matcherName, matcherMessage, additionalInfo);
}

// Matcher for `l.eq(r)`
export function bnToBeEq(l: BigNumberish, r: BigNumberish, additionalInfo?: string) {
    const comparator = (l: BigNumber, r: BigNumber) => l.eq(r);
    const matcherName = `bnToBeEq`;
    const matcherMessage = `equal to`;
    return matcherBody(l, r, comparator, matcherName, matcherMessage, additionalInfo);
}

// Matcher for `l.lt(r)`
export function bnToBeLt(l: BigNumberish, r: BigNumberish, additionalInfo?: string) {
    const comparator = (l: BigNumber, r: BigNumber) => l.lt(r);
    const matcherName = `bnToBeLt`;
    const matcherMessage = `less than`;
    return matcherBody(l, r, comparator, matcherName, matcherMessage, additionalInfo);
}

// Matcher for `l.lte(r)`
export function bnToBeLte(l: BigNumberish, r: BigNumberish, additionalInfo?: string) {
    const comparator = (l: BigNumber, r: BigNumber) => l.lte(r);
    const matcherName = `bnToBeLte`;
    const matcherMessage = `less than or equal`;
    return matcherBody(l, r, comparator, matcherName, matcherMessage, additionalInfo);
}

/**
 * Generic body of the BigNumber matchers. Use to reduce the amount of boilerplate code.
 *
 * @param l Initial number (from `expect(l)`).
 * @param r Number to compare to (from `.bnToBeXXX(r)`).
 * @param comparator Comparator function to invoke to see if test passes (e.g. `(l, r) => l.gt(r)`).
 * @param matcherName Name of the matcher function (e.g. `bnToBeGt`).
 * @param matcherMessage Generic part of the failure message (e.g. `greater than`).
 * @param additionalInfo Message provided by user to be included in case of failure.
 * @returns Object expected by jest matcher.
 */
function matcherBody(
    l: BigNumberish,
    r: BigNumberish,
    comparator: (l: BigNumber, r: BigNumber) => boolean,
    matcherName: string,
    matcherMessage: string,
    additionalInfo?: string
) {
    // Numbers are provided as `BigNumberish`, so they can be strings or numbers.
    const left = BigNumber.from(l);
    const right = BigNumber.from(r);
    const pass = comparator(left, right);

    // Declare messages for normal case and case where matcher was preceded by `.not`.
    let passMessage = new TestMessage()
        .matcherHint(`.not.${matcherName}`)
        .line('Expected the following number:')
        .received(left)
        .line(`to not be ${matcherMessage}:`)
        .expected(right)
        .additional(additionalInfo)
        .build();

    let failMessage = new TestMessage()
        .matcherHint(`.${matcherName}`)
        .line('Expected the following number:')
        .received(left)
        .line(`to be ${matcherMessage}:`)
        .expected(right)
        .additional(additionalInfo)
        .build();

    return {
        pass,
        message: () => (pass ? passMessage : failMessage)
    };
}

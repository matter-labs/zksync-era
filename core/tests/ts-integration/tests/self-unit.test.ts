/**
 * This file contains unit tests for the framework itself.
 * It does not receive a funced account and should not interact with the ZKsync server.
 */
import { TestMaster } from '../src/index';
import { BigNumber } from 'ethers';

describe('Common checks for library invariants', () => {
    test('Should not have a test master', () => {
        // Should not receive a test account in the unit tests file.
        expect(() => TestMaster.getInstance(__filename)).toThrow('Wallet for self-unit.test.ts suite was not provided');
    });

    test('BigNumber matchers should work', () => {
        const hundred = BigNumber.from(100);

        // gt
        expect(hundred).bnToBeGt(0);
        expect(hundred).not.bnToBeGt(100);

        // gte
        expect(hundred).bnToBeGte(0);
        expect(hundred).bnToBeGte(100);
        expect(hundred).not.bnToBeGte(200);

        // eq
        expect(hundred).bnToBeEq(100);
        expect(hundred).not.bnToBeEq(200);

        // lte
        expect(hundred).not.bnToBeLte(90);
        expect(hundred).bnToBeLte(100);
        expect(hundred).bnToBeLte(101);

        // lt
        expect(hundred).not.bnToBeLt(100);
        expect(hundred).bnToBeLt(101);
    });
});

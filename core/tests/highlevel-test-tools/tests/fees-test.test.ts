import { describe, it } from 'vitest';
import { createChainAndStartServer, feesTest, TESTED_CHAIN_TYPE } from '../src';

describe('Fees Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'Fees Test');

        await testChain.generateRealisticLoad();

        await testChain.waitForAllBatchesToBeExecuted();

        await testChain.mainNode.kill();

        await feesTest(testChain.chainName);
    });
});

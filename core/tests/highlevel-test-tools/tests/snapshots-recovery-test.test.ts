import { describe, it } from 'vitest';
import { createChainAndStartServer, snapshotsRecoveryTest, TESTED_CHAIN_TYPE } from '../src';

describe('Snapshot Recovery Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'Snapshot Recovery Test');

        await testChain.generateRealisticLoad();

        await testChain.waitForAllBatchesToBeExecuted();

        await testChain.initExternalNode();

        await snapshotsRecoveryTest(testChain.chainName);
    });
});

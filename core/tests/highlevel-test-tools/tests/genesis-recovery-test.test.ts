import { describe, it } from 'vitest';
import { createChainAndStartServer, TESTED_CHAIN_TYPE } from '../src';
import { genesisRecoveryTest } from '../src/run-integration-tests';

describe('Genesis Recovery Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'Genesis Recovery Test');

        await testChain.generateRealisticLoad();

        await testChain.waitForAllBatchesToBeExecuted();

        await testChain.initExternalNode();

        await genesisRecoveryTest(testChain.chainName);
    });
});

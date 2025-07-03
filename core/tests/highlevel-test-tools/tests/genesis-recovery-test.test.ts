import { describe, it } from 'vitest';
import { createChainAndStartServer, TESTED_CHAIN_TYPE } from '../src';
import { genesisRecoveryTest } from '../src/run-integration-tests';

describe('Genesis Recovery Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'Genesis Recovery Test');

        await testChain.generateRealisticLoad();

        await testChain.waitForAllBatchesToBeExecuted();

        await testChain.initExternalNode();

        // Run external node and store it in the testChain.externalNode field
        await testChain.runExternalNode();

        // Now we can access the external node through testChain.externalNode
        // For example: await testChain.externalNode?.kill();

        await genesisRecoveryTest(testChain.chainName);
    });
});

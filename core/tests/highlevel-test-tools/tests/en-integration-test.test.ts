import { describe, it } from 'vitest';
import { createChainAndStartServer, TESTED_CHAIN_TYPE } from '../src';
import { enIntegrationTests } from '../src/run-integration-tests';

describe('External Node Integration tests Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'External Node Integration tests Test');
        // Define some chain B used for interop tests
        const secondChainType = TESTED_CHAIN_TYPE === 'era' ? 'validium' : 'era';
        const testSecondChain = await createChainAndStartServer(secondChainType, 'External Node Integration tests');

        await testChain.generateRealisticLoad();
        await testSecondChain.generateRealisticLoad();

        await testChain.waitForAllBatchesToBeExecuted();
        await testSecondChain.waitForAllBatchesToBeExecuted();

        await testChain.initExternalNode();
        await testSecondChain.initExternalNode();

        await testChain.runExternalNode();
        await testSecondChain.runExternalNode();

        await enIntegrationTests(testChain.chainName, testSecondChain.chainName);
    });
});

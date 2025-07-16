import { describe, it } from 'vitest';
import { createChainAndStartServer, TESTED_CHAIN_TYPE } from '../src';
import { enIntegrationTests } from '../src/run-integration-tests';

describe('External Node Integration tests Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'External Node Integration tests Test');
        // Define some chain B used for interop tests
        const testSecondChain = await createChainAndStartServer('era', 'External Node Integration tests');

        // await testChain.generateRealisticLoad();

        // await testChain.waitForAllBatchesToBeExecuted();

        // await testChain.initExternalNode();

        // await testChain.runExternalNode();

        // await enIntegrationTests(testChain.chainName, testSecondChain.chainName);
    });
});

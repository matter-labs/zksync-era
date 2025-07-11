import { describe, it } from 'vitest';
import { TESTED_CHAIN_TYPE, createChainAndStartServer, runIntegrationTests } from '../src';

describe('Integration Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'Main Node Integration Test');
        // Define some chain B used for interop tests
        const testSecondChain = await createChainAndStartServer('era', 'Main Node Integration Test');

        await runIntegrationTests(testChain.chainName, testSecondChain.chainName);
    });
});

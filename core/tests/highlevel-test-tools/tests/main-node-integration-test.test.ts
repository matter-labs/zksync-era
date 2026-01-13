import { describe, it } from 'vitest';
import { TESTED_CHAIN_TYPE, createChainAndStartServer, runIntegrationTests } from '../src';

describe('Integration Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'Main Node Integration Test');
        // Define some chain B used for interop tests, only used when settling on gateway
        let secondChainName;
        if (process.env.USE_GATEWAY_CHAIN === 'WITH_GATEWAY') {
            const secondChainType = TESTED_CHAIN_TYPE === 'validium' ? 'custom_token' : 'era';
            const testSecondChain = await createChainAndStartServer(secondChainType, 'Main Node Integration Test');
            secondChainName = testSecondChain.chainName;
        }

        await runIntegrationTests(testChain.chainName, secondChainName);
    });
});

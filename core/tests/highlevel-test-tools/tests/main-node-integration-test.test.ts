import { describe, it } from 'vitest';
import { TESTED_CHAIN_TYPE, createChainAndStartServer, runIntegrationTests } from '../src';

describe('Integration Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const { chainName } = await createChainAndStartServer(TESTED_CHAIN_TYPE);

        await runIntegrationTests(chainName);
    });
});

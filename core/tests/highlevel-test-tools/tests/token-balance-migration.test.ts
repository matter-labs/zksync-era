import { describe, it } from 'vitest';
import { createChainAndStartServer, TESTED_CHAIN_TYPE, tokenBalanceMigrationTest } from '../src';

const useGatewayChain = process.env.USE_GATEWAY_CHAIN;
const shouldSkip = useGatewayChain !== 'WITH_GATEWAY';

if (shouldSkip) {
    console.log(
        `⏭️ Skipping asset migration test for ${TESTED_CHAIN_TYPE} chain (USE_GATEWAY_CHAIN=${useGatewayChain})`
    );
}

(shouldSkip ? describe.skip : describe)('Asset Migration Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'Asset Migration Test');

        await testChain.generateRealisticLoad();

        await testChain.waitForAllBatchesToBeExecuted();

        await tokenBalanceMigrationTest(testChain.chainName);
    });
});

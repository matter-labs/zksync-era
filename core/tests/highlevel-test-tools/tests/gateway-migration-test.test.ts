import { describe, it } from 'vitest';
import {
    createChainAndStartServer,
    TESTED_CHAIN_TYPE,
    gatewayMigrationToGatewayTest,
    gatewayMigrationFromGatewayTest
} from '../src';

const useGatewayChain = process.env.USE_GATEWAY_CHAIN;
const shouldSkip = useGatewayChain !== 'WITH_GATEWAY';

if (shouldSkip) {
    console.log(
        `⏭️ Skipping gateway migration test for ${TESTED_CHAIN_TYPE} chain (USE_GATEWAY_CHAIN=${useGatewayChain})`
    );
}

(shouldSkip ? describe.skip : describe)('Gateway Migration Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'Gateway Migration Test');

        await testChain.generateRealisticLoad();

        await testChain.waitForAllBatchesToBeExecuted();

        await testChain.mainNode.kill();

        await gatewayMigrationFromGatewayTest(testChain.chainName);

        await gatewayMigrationToGatewayTest(testChain.chainName);
    });
});

import {describe, it} from 'vitest';
import {
    type ChainType,
    createChainAndStartServer,
    generateRealisticLoad,
    waitForAllBatchesToBeExecuted,
    initExternalNode,
    ALL_CHAIN_TYPES, migrateToGatewayIfNeeded
} from '../src';
import {genesisRecoveryTest} from "../src/run-integration-tests";

describe('Genesis Recovery Test', () => {
    it.concurrent.each<ChainType>(ALL_CHAIN_TYPES)('for %s chain', async (chainType) => {
        const { chainName} = await createChainAndStartServer(chainType);

        await migrateToGatewayIfNeeded(chainName);

        await generateRealisticLoad(chainName);

        await waitForAllBatchesToBeExecuted(chainName);

        await initExternalNode(chainName);

        await genesisRecoveryTest(chainName);
    });
});

import {describe, it} from "vitest";
import {ALL_CHAIN_TYPES, ChainType, createChainAndStartServer, runIntegrationTests} from "../src";

describe('Integration Test', () => {
    it.concurrent.each<ChainType>(ALL_CHAIN_TYPES)('for %s chain', async (chainType) => {
        const { chainName } = await createChainAndStartServer(chainType);

        await runIntegrationTests(chainName);
    });
});

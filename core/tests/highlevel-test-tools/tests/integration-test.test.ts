import {describe, it} from "vitest";
import {CHAIN_TYPES, ChainType, createChainAndStartServer, runIntegrationTests} from "../src";

describe('Integration Test', () => {
    it.concurrent.each<ChainType>(CHAIN_TYPES)('for %s chain', async (chainType) => {
        const chain = await createChainAndStartServer(chainType);

        await runIntegrationTests(chain.chainName)

    });
});

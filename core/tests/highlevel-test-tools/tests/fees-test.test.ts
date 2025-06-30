import {describe, it} from 'vitest';
import {
  type ChainType,
  createChainAndStartServer,
  generateRealisticLoad,
  waitForAllBatchesToBeExecuted,
  feesTest,
  ALL_CHAIN_TYPES, migrateToGatewayIfNeeded
} from '../src';

describe('Fees Test', () => {
  it.concurrent.each<ChainType>(ALL_CHAIN_TYPES)('for %s chain', async (chainType) => {
    const { chainName, serverHandle } = await createChainAndStartServer(chainType);

    await migrateToGatewayIfNeeded(chainName);

    await generateRealisticLoad(chainName);

    await waitForAllBatchesToBeExecuted(chainName);

    await serverHandle.kill();

    await feesTest(chainName);
  });
});

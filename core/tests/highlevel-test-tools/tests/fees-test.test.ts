import {describe, it} from 'vitest';
import {
  type ChainType,
  createChainAndStartServer,
  generateRealisticLoad,
  waitForAllBatchesToBeExecuted,
  feesTest,
  CHAIN_TYPES
} from '../src';

describe('Fees Test', () => {
  it.concurrent.each<ChainType>(CHAIN_TYPES)('for %s chain', async (chainType) => {
    const { chainName, serverHandle } = await createChainAndStartServer(chainType);

    await generateRealisticLoad(chainName);

    await waitForAllBatchesToBeExecuted(chainName);

    await serverHandle.kill();

    await feesTest(chainName)

  });
});

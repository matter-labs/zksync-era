import {describe, it} from 'vitest';
import {
  type ChainType,
  createChainAndStartServer,
  generateRealisticLoad,
  waitForAllBatchesToBeExecuted,
  snapshotsRecoveryTest,
  ALL_CHAIN_TYPES, initExternalNode
} from '../src';

describe('Snapshot Recovery Test', () => {
  it.concurrent.each<ChainType>(ALL_CHAIN_TYPES)('for %s chain', async (chainType) => {
    const { chainName} = await createChainAndStartServer(chainType);

    await generateRealisticLoad(chainName);

    await waitForAllBatchesToBeExecuted(chainName);

    await initExternalNode(chainName);

    await snapshotsRecoveryTest(chainName);

  });
}); 

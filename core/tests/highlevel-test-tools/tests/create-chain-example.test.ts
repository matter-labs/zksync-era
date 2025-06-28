import { describe, it } from 'vitest';
import { exec } from 'child_process';
import { promisify } from 'util';
import {
  cleanHistoricalLogs,
  cleanTestChains,
  cleanMutexLockFiles,
  createChain,
  runIntegrationTests,
  type ChainType
} from '../src';
import { getRpcUrl, queryJsonRpc, getL1BatchNumber, getL1BatchDetails } from '../src/rpc-utils';
import {feesTest, revertTest, upgradeTest} from "../src/run-integration-tests";
import {setupDbTemplate} from "../src/create-chain";

const execAsync = promisify(exec);

const CHAIN_TYPES: ChainType[] = [
  'consensus',
  'validium',
  'da_migration',
  'custom_token'
];

async function getChainId(chainName: string): Promise<number> {
  const rpcUrl = getRpcUrl(chainName);
  console.log(`🔗 Querying chain ID from RPC URL: ${rpcUrl}`);
  
  try {
    const chainId = await queryJsonRpc(rpcUrl, 'eth_chainId');
    console.log(`📋 Chain ID from RPC: ${chainId}`);
    return parseInt(chainId, 16);
  } catch (error) {
    console.error(`❌ Failed to query chain ID: ${error}`);
    throw error;
  }
}

async function waitForL1BatchExecution(chainName: string, timeoutMs: number = 300000): Promise<any> {
  const batchNumber = await getL1BatchNumber(chainName);
  console.log(`⏳ Waiting for L1 batch ${batchNumber} execution (executeTxHash to become not null)...`);
  
  const startTime = Date.now();
  const maxWaitTime = timeoutMs;
  
  while (Date.now() - startTime < maxWaitTime) {
    try {
      const l1BatchDetails = await getL1BatchDetails(chainName, batchNumber);
      
      if (l1BatchDetails.executeTxHash) {
        console.log(`✅ L1 batch ${batchNumber} executed successfully. ExecuteTxHash: ${l1BatchDetails.executeTxHash}`);
        return l1BatchDetails;
      }
      
      await new Promise(resolve => setTimeout(resolve, 5000));
    } catch (error) {
      console.log(`⚠️ Error checking L1 batch execution status, retrying... Error: ${error}`);
      await new Promise(resolve => setTimeout(resolve, 5000));
    }
  }
  
  throw new Error(`Timeout waiting for L1 batch ${batchNumber} execution after ${timeoutMs}ms`);
}

describe('Chain Creation Tests', () => {
  beforeAll(async () => {
    try {
      await execAsync('pkill zksync_server');
    } catch (error) {}

    cleanHistoricalLogs();
    cleanTestChains('../../.././chains');
    cleanMutexLockFiles();
    await setupDbTemplate()
  });

  it.concurrent.each<ChainType>(CHAIN_TYPES)('fee test for %s chain', async (chainType) => {
    const { chainId, config, serverHandle } = await createChain(chainType);

    await runIntegrationTests(chainId, 'ETH token checks');

    await waitForL1BatchExecution(chainId);

    await serverHandle.kill();

    await feesTest(chainId)

  }, 600 * 1000);
});

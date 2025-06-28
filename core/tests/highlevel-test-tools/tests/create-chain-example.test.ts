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

async function waitForL1BatchExecution(chainName: string, batchNumber: number, timeoutMs: number = 300000): Promise<any> {
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
    // Kill any existing zksync_server processes
    try {
      await execAsync('pkill zksync_server');
    } catch (error) {
      // Ignore errors if no processes were found
    }

    // Clean logs and test chains at the start of this test suite
    cleanHistoricalLogs();
    cleanTestChains('../../.././chains');

    // Clean up any leftover mutex lock files from previous test runs
    cleanMutexLockFiles();

    await setupDbTemplate()
  });

  it.concurrent.each<ChainType>(CHAIN_TYPES)('should create and start %s chain and run integration tests', async (chainType) => {
    const { chainId, config, serverHandle } = await createChain(chainType);

    console.log(`Created ${chainType} chain with ID: ${chainId}`);
    console.log(`Chain config:`, config);

    // Query and print the actual chainId from the RPC endpoint
    const actualChainId = await getChainId(chainId);
    console.log(`🔍 Actual chain ID from RPC: ${actualChainId}`);

    // //Run integration tests for the created chain
    console.log(`🧪 Running integration tests for ${chainType} chain: ${chainId}`);
    await runIntegrationTests(chainId, 'ETH token checks');
    //
    // // Query and print the L1 batch number
    const l1BatchNumber = await getL1BatchNumber(chainId);
    console.log(`📦 L1 batch number: ${l1BatchNumber}`);
    //
    // // Wait for L1 batch execution and print details
    console.log(`🔍 Waiting for L1 batch ${l1BatchNumber} execution...`);
    await waitForL1BatchExecution(chainId, l1BatchNumber);
    //
    // Kill the server before starting upgrade test
    await serverHandle.kill();

    // await feesTest(chainId)

  }, 600000); // 10 minutes timeout

  // it('should create 10 consensus chains in parallel with mutex protection', async () => {
  //   // This test creates 10 consensus chains in parallel to test the mutex functionality
  //   // under high concurrency. The mutex should ensure only one chain is in phase1
  //   // initialization at a time, while phase2 can run concurrently.
  //   const chainCount = 10;
  //   const startTime = Date.now();
  //
  //   console.log(`Starting parallel creation of ${chainCount} consensus chains...`);
  //
  //   const chainPromises = Array.from({ length: chainCount }, (_, index) =>
  //     createChain('consensus').then(({ chainId }) => {
  //       console.log(`Chain ${index + 1}/${chainCount} created: ${chainId}`);
  //       return chainId;
  //     })
  //   );
  //
  //   const chainIds = await Promise.all(chainPromises);
  //   const endTime = Date.now();
  //   const duration = (endTime - startTime) / 1000;
  //
  //   console.log(`✅ Successfully created ${chainCount} consensus chains in ${duration.toFixed(2)} seconds`);
  //
  //   // Verify all chains were created successfully
  //   expect(chainIds).toHaveLength(chainCount);
  //
  //   // Verify all chain IDs follow the expected pattern
  //   chainIds.forEach((chainId, index) => {
  //     expect(chainId).toMatch(/^consensus_[a-f0-9]{8}$/);
  //     console.log(`Verified chain ${index + 1}: ${chainId}`);
  //   });
  //
  //   // Verify all chain IDs are unique
  //   const uniqueChainIds = new Set(chainIds);
  //   expect(uniqueChainIds.size).toBe(chainCount);
  //
  //   console.log(`🎉 All ${chainCount} consensus chains created successfully with mutex protection!`);
  // }, 1800000); // 30 minutes timeout for parallel test

});

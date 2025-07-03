import { exec } from 'child_process';
import { promisify } from 'util';
import {
  cleanMutexLockFiles, 
  cleanTestChains 
} from './src';
import {cleanHistoricalLogs} from "./src/logs";

const execAsync = promisify(exec);

/**
 * Cleanup function to kill any remaining zksync_server processes
 */
async function cleanup(): Promise<void> {
  try {
    console.log('🛑 Killing any remaining zksync_server processes...');
    await execAsync('pkill zksync_server');
  } catch (error) {
    // Ignore errors if no processes were found
    console.log('ℹ️ No remaining zksync_server processes found');
  }
}

/**
 * Global setup function that runs once before all tests
 * This is called by Vitest's globalSetup configuration
 */
export default async function globalSetup(): Promise<void> {
  console.log('🔧 Running global test setup...');
  
  // Set up cleanup handlers for graceful shutdown
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received SIGINT, cleaning up...');
    await cleanup();
    process.exit(0);
  });
  
  process.on('SIGTERM', async () => {
    console.log('\n🛑 Received SIGTERM, cleaning up...');
    await cleanup();
    process.exit(0);
  });
  
  process.on('exit', async () => {
    console.log('🧹 Running final cleanup...');
    await cleanup();
  });
  
  try {
    // Kill any existing zksync_server processes
    console.log('🛑 Killing existing zksync_server processes...');
    await execAsync('pkill zksync_server');
  } catch (error) {
    // Ignore errors if no processes were found
    console.log('ℹ️ No existing zksync_server processes found');
  }

  // Clean historical logs
  console.log('🧹 Cleaning historical logs...');
  cleanHistoricalLogs();
  
  // Clean test chains
  console.log('🧹 Cleaning test chains...');
  cleanTestChains('../../.././chains');
  
  // Clean mutex lock files
  console.log('🧹 Cleaning mutex lock files...');
  cleanMutexLockFiles();
  
  console.log('✅ Global test setup completed');
}

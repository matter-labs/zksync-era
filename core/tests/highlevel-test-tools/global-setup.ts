import { exec } from 'child_process';
import { promisify } from 'util';
import { cleanMutexLockFiles, cleanTestChains } from './src';
import { cleanHistoricalLogs } from './src/logs';
import { chainsPath } from './src/zksync-home';

const execAsync = promisify(exec);

/**
 * Cleanup function to kill any remaining zksync_server processes
 */
async function cleanup(): Promise<void> {
    for (const processName of ['zksync_server', 'external_node']) {
        try {
            console.log(`🛑 Killing any remaining ${processName} processes...`);
            await execAsync(`pkill ${processName}`);
        } catch {
            // Ignore errors if no processes were found.
            console.log(`ℹ️ No remaining ${processName} processes found`);
        }
    }
}

/**
 * Global setup function that runs once before all tests
 * This is called by Vitest's globalSetup configuration
 */
export default async function globalSetup(): Promise<() => Promise<void>> {
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

    const inCi = process.env.CI;
    if (inCi !== '1') {
        await cleanup();
    }

    // Clean historical logs
    console.log('🧹 Cleaning historical logs...');
    cleanHistoricalLogs();

    // Clean test chains
    console.log('🧹 Cleaning test chains...');
    cleanTestChains(chainsPath());

    // Clean mutex lock files
    console.log('🧹 Cleaning mutex lock files...');
    cleanMutexLockFiles();

    console.log('✅ Global test setup completed');

    // Vitest awaits a teardown returned by globalSetup. An async `exit` handler is not awaited by
    // Node.js and used to leave server / external-node processes alive between jobs or retries.
    return async () => {
        console.log('🧹 Running final cleanup...');
        await cleanup();
    };
}

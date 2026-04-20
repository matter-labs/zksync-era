import { executeCommand } from './execute-command';
import { FileMutex } from './file-mutex';

/**
 * Global mutex for gateway migration to prevent concurrent migrations
 */
const gatewayMutex = new FileMutex();

/**
 * Constants for migration readiness check
 */
const GATEWAY_CHAIN_NAME = 'gateway';
const GATEWAY_MIGRATION_CONTEXT = 'gateway_migration';

function shouldUseGatewayChain(chainName: string, action: string): boolean {
    const useGatewayChain = process.env.USE_GATEWAY_CHAIN;
    if (useGatewayChain !== 'WITH_GATEWAY') {
        console.log(`⏭️ Skipping ${action} for ${chainName} (USE_GATEWAY_CHAIN=${useGatewayChain})`);
        return false;
    }

    return true;
}

async function runGatewayCommand(chainName: string, args: string[]): Promise<void> {
    await executeCommand('zkstack', ['chain', 'gateway', ...args], chainName, GATEWAY_MIGRATION_CONTEXT);
}

async function withGatewayMutex(chainName: string, stage: string, action: () => Promise<void>): Promise<void> {
    console.log(`🔒 Acquiring mutex for ${stage} of ${chainName}...`);
    await gatewayMutex.acquire();
    console.log(`✅ Mutex acquired for ${stage} of ${chainName}`);

    try {
        await action();
    } finally {
        gatewayMutex.release();
    }
}

/**
 * Migrates a chain to gateway if the USE_GATEWAY_CHAIN environment variable is set to 'WITH_GATEWAY'
 * Uses file mutex to ensure only one gateway migration happens at a time
 * @param chainName - The name of the chain to migrate
 * @returns Promise that resolves when migration is complete
 */
export async function migrateToGatewayIfNeeded(chainName: string): Promise<void> {
    if (!shouldUseGatewayChain(chainName, 'gateway migration')) {
        return;
    }

    console.log(`🔄 Migrating chain ${chainName} to gateway...`);

    try {
        await withGatewayMutex(chainName, 'gateway migration', async () => {
            await runGatewayCommand(chainName, [
                'migrate-to-gateway',
                '--chain',
                chainName,
                '--gateway-chain-name',
                GATEWAY_CHAIN_NAME
            ]);
            console.log(`✅ Successfully migrated chain ${chainName} to gateway`);
        });
    } catch (error) {
        console.error(`❌ Failed to migrate chain ${chainName} to gateway:`, error);
        throw error;
    }
}

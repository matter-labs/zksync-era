import { executeCommand } from './execute-command';
import { FileMutex } from './file-mutex';

/**
 * Global mutex for gateway migration to prevent concurrent migrations
 */
const gatewayMutex = new FileMutex();

/**
 * Migrates a chain to gateway if the USE_GATEWAY_CHAIN environment variable is set to 'WITH_GATEWAY'
 * Uses file mutex to ensure only one gateway migration happens at a time
 * @param chainName - The name of the chain to migrate
 * @returns Promise that resolves when migration is complete
 */
export async function migrateToGatewayIfNeeded(chainName: string): Promise<void> {
    const useGatewayChain = process.env.USE_GATEWAY_CHAIN;

    if (useGatewayChain !== 'WITH_GATEWAY') {
        console.log(`⏭️ Skipping gateway migration for ${chainName} (USE_GATEWAY_CHAIN=${useGatewayChain})`);
        return;
    }

    console.log(`🔄 Migrating chain ${chainName} to gateway...`);

    try {
        // Acquire mutex for gateway migration
        console.log(`🔒 Acquiring mutex for gateway migration of ${chainName}...`);
        await gatewayMutex.acquire();
        console.log(`✅ Mutex acquired for gateway migration of ${chainName}`);

        try {
            await executeCommand(
                'zkstack',
                ['chain', 'gateway', 'migrate-to-gateway', '--chain', chainName, '--gateway-chain-name', 'gateway'],
                chainName,
                'gateway_migration'
            );

            await executeCommand(
                'zkstack',
                [
                    'chain',
                    'gateway',
                    'finalize-chain-migration-to-gateway',
                    '--chain',
                    chainName,
                    '--gateway-chain-name',
                    'gateway',
                    '--deploy-paymaster'
                ],
                chainName,
                'gateway_migration'
            );

            console.log(`✅ Successfully migrated chain ${chainName} to gateway`);
        } finally {
            // Always release the mutex
            gatewayMutex.release();
        }
    } catch (error) {
        console.error(`❌ Failed to migrate chain ${chainName} to gateway:`, error);
        throw error;
    }
}

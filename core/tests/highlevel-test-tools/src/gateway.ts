import { executeCommand } from './execute-command';
import { FileMutex } from './file-mutex';
import { findHome } from './zksync-home';
import * as utils from 'utils';
import { GW_ASSET_TRACKER_ADDRESS, ArtifactWrappedBaseToken, ArtifactGWAssetTracker } from 'utils/src/constants';
import { loadConfig } from 'utils/build/file-configs';
import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';

/**
 * Global mutex for gateway migration to prevent concurrent migrations
 */
const gatewayMutex = new FileMutex();
/**
 * Constants for migration readiness check
 */
const MIGRATION_STARTED_TOPIC = ethers.id('MigrationStarted(uint256,bytes32,uint256)');
const BLOCK_SEARCH_RANGE = 5000;
const MAX_BLOCK_LOOKBACK = 200000;

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

            console.log(`✅ Successfully migrated chain ${chainName} to gateway`);
        } finally {
            // Always release the mutex
            gatewayMutex.release();
        }
    } catch (error) {
        console.error(`❌ Failed to migrate chain ${chainName} to gateway:`, error);
        throw error;
    }

    // Wait until the migration is ready to finalize without holding the mutex.
    await waitForMigrationReadyForFinalize(chainName);

    try {
        // Acquire mutex for finalizing gateway migration
        console.log(`🔒 Acquiring mutex for finalizing gateway migration of ${chainName}...`);
        await gatewayMutex.acquire();
        console.log(`✅ Mutex acquired for finalizing gateway migration of ${chainName}`);

        try {
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

            console.log(`✅ Successfully finalized migration of chain ${chainName} to gateway`);
        } finally {
            // Always release the mutex
            gatewayMutex.release();
        }
    } catch (error) {
        console.error(`❌ Failed to finalize migration of chain ${chainName} to gateway:`, error);
        throw error;
    }
}

/**
 * Sets up the payment of settlement fees for a chain
 * @dev On CI, Gateway is an ETH-based chain, meaning settlement fees are paid in wrapped ETH
 * @param chainName - The name of the chain to set up payment of settlement fees for
 * @returns Promise that resolves when set up of settlement fees is complete
 */
export async function agreeToPaySettlementFees(chainName: string): Promise<void> {
    const pathToHome = findHome();
    const gatewayGeneralConfig = loadConfig({
        pathToHome,
        chain: 'gateway',
        config: 'general.yaml'
    });
    const genesisConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'genesis.yaml'
    });
    const secretsConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'secrets.yaml'
    });
    const walletsConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'wallets.yaml'
    });

    const l2ChainId = Number(genesisConfig?.l2_chain_id);
    const l1RpcUrl = secretsConfig.l1.l1_rpc_url;
    const gatewayRpcUrl = gatewayGeneralConfig.api.web3_json_rpc.http_url;
    const operator = new zksync.Wallet(
        walletsConfig.operator.private_key,
        new zksync.Provider(gatewayRpcUrl),
        new ethers.JsonRpcProvider(l1RpcUrl)
    );
    const gwAssetTracker = new zksync.Contract(GW_ASSET_TRACKER_ADDRESS, ArtifactGWAssetTracker.abi, operator);

    const gwWrappedZkTokenAddress = await gwAssetTracker.wrappedZKToken();
    const gwWrappedZkToken = new zksync.Contract(gwWrappedZkTokenAddress, ArtifactWrappedBaseToken.abi, operator);

    // Wrap 1 ETH to the operator, approve spending to GWAT, and agree to pay settlement fees for the chain
    await (await gwWrappedZkToken.deposit({ value: ethers.parseEther('1') })).wait();
    await (await gwWrappedZkToken.approve(GW_ASSET_TRACKER_ADDRESS, ethers.parseEther('1'))).wait();
    await (await gwAssetTracker.agreeToPaySettlementFees(l2ChainId)).wait();
}

function loadMigrationFinalizeCheckConfig(chainName: string) {
    const pathToHome = findHome();
    const contractsConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'contracts.yaml'
    });
    const secretsConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'secrets.yaml'
    });
    const genesisConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'genesis.yaml'
    });
    const gatewayContractsConfig = loadConfig({
        pathToHome,
        chain: 'gateway',
        config: 'contracts.yaml'
    });

    const l1RpcUrl = secretsConfig?.l1?.l1_rpc_url;
    const gatewayRpcUrl = secretsConfig?.l1?.gateway_rpc_url;
    const bridgehubProxyAddr = contractsConfig?.ecosystem_contracts?.bridgehub_proxy_addr;
    const gatewayDiamondProxyAddr = gatewayContractsConfig?.l1?.diamond_proxy_addr;
    const l2ChainId = Number(genesisConfig?.l2_chain_id);

    if (!l1RpcUrl || !gatewayRpcUrl || !bridgehubProxyAddr || !gatewayDiamondProxyAddr || !Number.isFinite(l2ChainId)) {
        throw new Error(
            `Missing gateway migration config for chain ${chainName} ` +
                `(l1RpcUrl=${!!l1RpcUrl}, gatewayRpcUrl=${!!gatewayRpcUrl}, bridgehubProxyAddr=${!!bridgehubProxyAddr}, ` +
                `gatewayDiamondProxyAddr=${!!gatewayDiamondProxyAddr}, l2ChainId=${l2ChainId})`
        );
    }

    return {
        l1RpcUrl,
        gatewayRpcUrl,
        l2ChainId,
        bridgehubProxyAddr,
        gatewayDiamondProxyAddr
    };
}

async function findLatestMigrationTxHash(
    l1Provider: ethers.JsonRpcProvider,
    chainAssetHandlerAddr: string,
    l2ChainId: number
): Promise<string | null> {
    const latestBlock = await l1Provider.getBlockNumber();
    const chainIdTopic = ethers.zeroPadValue(ethers.toBeHex(l2ChainId), 32);

    for (
        let toBlock = latestBlock;
        toBlock >= 0 && latestBlock - toBlock <= MAX_BLOCK_LOOKBACK;
        toBlock -= BLOCK_SEARCH_RANGE
    ) {
        const fromBlock = Math.max(0, toBlock - BLOCK_SEARCH_RANGE + 1);
        const logs = await l1Provider.getLogs({
            address: chainAssetHandlerAddr,
            topics: [MIGRATION_STARTED_TOPIC, chainIdTopic],
            fromBlock,
            toBlock
        });

        if (logs.length > 0) {
            return logs[logs.length - 1].transactionHash;
        }
    }

    return null;
}

async function isMigrationReadyForFinalize(chainName: string): Promise<boolean> {
    const config = loadMigrationFinalizeCheckConfig(chainName);
    const l1Provider = new ethers.JsonRpcProvider(config.l1RpcUrl);
    const gatewayProvider = new zksync.Provider(config.gatewayRpcUrl);
    const bridgehub = new ethers.Contract(
        config.bridgehubProxyAddr,
        ['function chainAssetHandler() view returns (address)'],
        l1Provider
    );
    const chainAssetHandlerAddr = await bridgehub.chainAssetHandler();

    const migrationTxHash = await findLatestMigrationTxHash(l1Provider, chainAssetHandlerAddr, config.l2ChainId);
    if (!migrationTxHash) {
        return false;
    }
    const receipt = await l1Provider.getTransactionReceipt(migrationTxHash);
    if (!receipt) {
        return false;
    }

    const gatewayMainContract = await gatewayProvider.getMainContractAddress();
    const priorityOpHash = zksync.utils.getL2HashFromPriorityOp(receipt, gatewayMainContract);
    const l2Receipt = await gatewayProvider.getTransactionReceipt(priorityOpHash);
    if (!l2Receipt?.l1BatchNumber) {
        return false;
    }

    const gatewayDiamondProxy = new ethers.Contract(
        config.gatewayDiamondProxyAddr,
        ['function getTotalBatchesExecuted() view returns (uint256)'],
        l1Provider
    );

    const totalExecuted = BigInt(await gatewayDiamondProxy.getTotalBatchesExecuted());
    const batchNumber = BigInt(l2Receipt.l1BatchNumber);
    return totalExecuted >= batchNumber;
}

export async function waitForMigrationReadyForFinalize(chainName: string): Promise<void> {
    while (true) {
        try {
            if (await isMigrationReadyForFinalize(chainName)) {
                console.log(`✅ Migration is ready to finalize for ${chainName}`);
                return;
            }

            console.log(`⏳ Migration not ready to finalize for ${chainName}, retrying...`);
        } catch (error) {
            console.warn(`⚠️ Failed to check migration readiness for ${chainName}: ${error}`);
        }
        await utils.sleep(5);
    }
}

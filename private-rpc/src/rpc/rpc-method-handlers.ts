import {
    eth_call,
    eth_getBlockByHash,
    eth_getBlockByNumber,
    eth_getBlockReceipts,
    eth_getLogs,
    eth_getTransactionReceipt,
    eth_sendRawTransaction,
    forbiddenMethod,
    onlyCurrentUser,
    validatedEthereumCall,
    whoAmI,
    zks_getRawBlockTransactions,
    zks_sendRawTransactionWithDetailedOutput
} from '@/rpc/methods';
import { hexSchema } from '@/schemas/hex';
import { z } from 'zod';

export const allHandlers = [
    // Filter out debug_* methods
    forbiddenMethod('debug_traceBlockByHash'),
    forbiddenMethod('debug_traceBlockByNumber'),
    forbiddenMethod('debug_traceCall'),
    forbiddenMethod('debug_traceTransaction'),

    // Filter out en_* methods
    forbiddenMethod('en_syncL2Block'),
    forbiddenMethod('en_consensusGlobalConfig'),
    forbiddenMethod('en_blockMetadata'),
    forbiddenMethod('en_syncTokens'),
    forbiddenMethod('en_genesisConfig'),
    forbiddenMethod('en_whitelistedTokensForAA'),
    forbiddenMethod('en_getEcosystemContracts'),
    forbiddenMethod('en_getProtocolVersionInfo'),

    // Filter out snapshots_* methods
    forbiddenMethod('snapshots_getAllSnapshots'),
    forbiddenMethod('snapshots_getSnapshot'),

    // Filter out unstable_* methods
    forbiddenMethod('unstable_getTransactionExecutionInfo'),
    forbiddenMethod('unstable_getTeeProofs'),
    forbiddenMethod('unstable_getChainLogProof'),
    forbiddenMethod('unstable_unconfirmedTxsCount'),
    forbiddenMethod('unstable_getDataAvailabilityDetails'),
    forbiddenMethod('unstable_supportsUnsafeDepositFilter'),
    forbiddenMethod('unstable_l1ToL2TxsStatus'),
    forbiddenMethod('unstable_gatewayMigrationStatus'),

    // Filter out web3_* methods
    forbiddenMethod('web3_clientVersion'),

    // Filter out methods that reveal deployed code
    forbiddenMethod('eth_getCode'),
    forbiddenMethod('zks_getBytecodeByHash'),

    // Filter out methods that reveal blockchain state information
    forbiddenMethod('eth_accounts'),
    forbiddenMethod('eth_getStorageAt'),
    forbiddenMethod('eth_getTransactionByBlockHashAndIndex'),
    forbiddenMethod('eth_getTransactionByBlockNumberAndIndex'),
    forbiddenMethod('eth_newFilter'),
    forbiddenMethod('eth_newPendingTransactionFilter'),
    forbiddenMethod('zks_getProof'),

    // Restrict methods that require to be called only for the current user
    onlyCurrentUser('eth_getBalance', [z.union([hexSchema, z.string()])]),
    onlyCurrentUser('eth_getTransactionCount', [z.union([hexSchema, z.string()])]),
    onlyCurrentUser('zks_getAllAccountBalances'),

    // Methods with custom logic.
    validatedEthereumCall('eth_call'),
    validatedEthereumCall('eth_estimateGas'),
    validatedEthereumCall('zks_estimateFee'),
    validatedEthereumCall('zks_estimateGasL1ToL2'),
    eth_getBlockByHash,
    eth_getBlockByNumber,
    eth_getBlockReceipts,
    eth_getLogs,
    eth_getTransactionReceipt,
    eth_sendRawTransaction,
    whoAmI,
    zks_getRawBlockTransactions,
    zks_sendRawTransactionWithDetailedOutput
];

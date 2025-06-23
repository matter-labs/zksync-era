import {
    eth_getBlockByHash,
    eth_getBlockByNumber,
    eth_getBlockReceipts,
    eth_getLogs,
    eth_sendRawTransaction,
    forbiddenMethod,
    onlyCurrentUser,
    unrestricted,
    validatedEthereumCall,
    whoAmI,
    zks_getRawBlockTransactions,
    zks_sendRawTransactionWithDetailedOutput
} from '@/rpc/methods';
import { hexSchema } from '@/schemas/hex';
import { z } from 'zod';

export const allHandlers = [
    /* ─────────────────────────────
       Eth namespace (exact order)
       ───────────────────────────── */
    unrestricted('eth_blockNumber'),
    unrestricted('eth_chainId'),
    validatedEthereumCall('eth_call', 2),
    validatedEthereumCall('eth_estimateGas', 2),
    unrestricted('eth_gasPrice'),
    forbiddenMethod('eth_newFilter'),
    unrestricted('eth_newBlockFilter'),
    unrestricted('eth_uninstallFilter'),
    forbiddenMethod('eth_newPendingTransactionFilter'),
    eth_getLogs,
    unrestricted('eth_getFilterLogs'),
    unrestricted('eth_getFilterChanges'),
    onlyCurrentUser('eth_getBalance', [z.union([hexSchema, z.string()])]),
    eth_getBlockByNumber,
    eth_getBlockByHash,
    unrestricted('eth_getBlockTransactionCountByNumber'),
    eth_getBlockReceipts,
    unrestricted('eth_getBlockTransactionCountByHash'),
    forbiddenMethod('eth_getCode'),
    forbiddenMethod('eth_getStorageAt'),
    onlyCurrentUser('eth_getTransactionCount', [z.union([hexSchema, z.string()])]),
    unrestricted('eth_getTransactionByHash'),
    forbiddenMethod('eth_getTransactionByBlockHashAndIndex'),
    forbiddenMethod('eth_getTransactionByBlockNumberAndIndex'),
    unrestricted('eth_getTransactionReceipt'),
    unrestricted('eth_protocolVersion'),
    eth_sendRawTransaction,
    unrestricted('eth_syncing'),
    forbiddenMethod('eth_accounts'),
    unrestricted('eth_coinbase'),
    unrestricted('eth_getCompilers'),
    unrestricted('eth_hashrate'),
    unrestricted('eth_getUncleCountByBlockHash'),
    unrestricted('eth_getUncleCountByBlockNumber'),
    unrestricted('eth_mining'),
    unrestricted('eth_feeHistory'),
    unrestricted('eth_maxPriorityFeePerGas'),

    /* ─────────────────────────────
       Zks namespace (exact order)
       ───────────────────────────── */
    validatedEthereumCall('zks_estimateFee', 1),
    validatedEthereumCall('zks_estimateGasL1ToL2', 1),
    unrestricted('zks_getBridgehubContract'),
    unrestricted('zks_getMainContract'),
    unrestricted('zks_getL2Multicall3'),
    unrestricted('zks_getTestnetPaymaster'),
    unrestricted('zks_getTimestampAsserter'),
    unrestricted('zks_getBridgeContracts'),
    unrestricted('zks_getBaseTokenL1Address'),
    unrestricted('zks_L1ChainId'),
    unrestricted('zks_getL2ToL1LogProof'),
    unrestricted('zks_L1BatchNumber'),
    unrestricted('zks_getL1BatchBlockRange'),
    unrestricted('zks_getBlockDetails'),
    unrestricted('zks_getTransactionDetails'),
    zks_getRawBlockTransactions,
    unrestricted('zks_getL1BatchDetails'),
    forbiddenMethod('zks_getBytecodeByHash'),
    unrestricted('zks_getL1GasPrice'),
    unrestricted('zks_getFeeParams'),
    unrestricted('zks_getProtocolVersion'),
    forbiddenMethod('zks_getProof'),
    unrestricted('zks_getBatchFeeInput'),
    zks_sendRawTransactionWithDetailedOutput,

    whoAmI
];

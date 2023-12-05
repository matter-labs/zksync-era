import { ethers, BigNumber, BigNumberish, utils, providers, BytesLike } from 'ethers';
import Formatter = providers.Formatter;
import { ExternalProvider } from '@ethersproject/providers';
import { ConnectionInfo } from '@ethersproject/web';
import { Address, EventFilter, BlockTag, TransactionResponse, TransactionRequest, TransactionStatus, Token, PriorityOpResponse, BalancesMap, MessageProof, TransactionReceipt, Block, BlockWithTransactions, Log, TransactionDetails, BatchDetails, BlockDetails, ContractAccountInfo } from './types';
import { Signer } from './signer';
export declare class Provider extends ethers.providers.JsonRpcProvider {
    private static _nextPollId;
    protected contractAddresses: {
        mainContract?: Address;
        erc20BridgeL1?: Address;
        erc20BridgeL2?: Address;
        wethBridgeL1?: Address;
        wethBridgeL2?: Address;
    };
    poll(): Promise<void>;
    getTransactionReceipt(transactionHash: string | Promise<string>): Promise<TransactionReceipt>;
    getBlock(blockHashOrBlockTag: BlockTag | string | Promise<BlockTag | string>): Promise<Block>;
    getBlockWithTransactions(blockHashOrBlockTag: BlockTag | string | Promise<BlockTag | string>): Promise<BlockWithTransactions>;
    static getFormatter(): Formatter;
    getBalance(address: Address, blockTag?: BlockTag, tokenAddress?: Address): Promise<ethers.BigNumber>;
    l2TokenAddress(token: Address): Promise<string>;
    l1TokenAddress(token: Address): Promise<string>;
    static hexlifyTransaction(transaction: ethers.providers.TransactionRequest, allowExtra?: Record<string, boolean>): {
        [key: string]: string | ethers.utils.AccessList;
    };
    estimateGas(transaction: utils.Deferrable<TransactionRequest>): Promise<BigNumber>;
    estimateGasL1(transaction: utils.Deferrable<TransactionRequest>): Promise<BigNumber>;
    getGasPrice(token?: Address): Promise<BigNumber>;
    constructor(url?: ConnectionInfo | string, network?: ethers.providers.Networkish);
    getMessageProof(blockNumber: ethers.BigNumberish, sender: Address, messageHash: BytesLike, logIndex?: number): Promise<MessageProof | null>;
    getLogProof(txHash: BytesLike, index?: number): Promise<MessageProof | null>;
    getL1BatchBlockRange(l1BatchNumber: number): Promise<[number, number] | null>;
    getMainContractAddress(): Promise<Address>;
    getTestnetPaymasterAddress(): Promise<Address | null>;
    getDefaultBridgeAddresses(): Promise<{
        erc20L1: string;
        erc20L2: string;
        wethL1: string;
        wethL2: string;
    }>;
    getConfirmedTokens(start?: number, limit?: number): Promise<Token[]>;
    getTokenPrice(token: Address): Promise<string | null>;
    getAllAccountBalances(address: Address): Promise<BalancesMap>;
    l1ChainId(): Promise<number>;
    getL1BatchNumber(): Promise<number>;
    getL1BatchDetails(number: number): Promise<BatchDetails>;
    getBlockDetails(number: number): Promise<BlockDetails>;
    getTransactionDetails(txHash: BytesLike): Promise<TransactionDetails>;
    getWithdrawTx(transaction: {
        token: Address;
        amount: BigNumberish;
        from?: Address;
        to?: Address;
        bridgeAddress?: Address;
        overrides?: ethers.CallOverrides;
    }): Promise<ethers.providers.TransactionRequest>;
    estimateGasWithdraw(transaction: {
        token: Address;
        amount: BigNumberish;
        from?: Address;
        to?: Address;
        bridgeAddress?: Address;
        overrides?: ethers.CallOverrides;
    }): Promise<BigNumber>;
    getTransferTx(transaction: {
        to: Address;
        amount: BigNumberish;
        from?: Address;
        token?: Address;
        overrides?: ethers.CallOverrides;
    }): Promise<ethers.providers.TransactionRequest>;
    estimateGasTransfer(transaction: {
        to: Address;
        amount: BigNumberish;
        from?: Address;
        token?: Address;
        overrides?: ethers.CallOverrides;
    }): Promise<BigNumber>;
    static getDefaultProvider(): Provider;
    newFilter(filter: EventFilter | Promise<EventFilter>): Promise<BigNumber>;
    newBlockFilter(): Promise<BigNumber>;
    newPendingTransactionsFilter(): Promise<BigNumber>;
    getFilterChanges(idx: BigNumber): Promise<Array<Log | string>>;
    getLogs(filter?: EventFilter | Promise<EventFilter>): Promise<Array<Log>>;
    protected _parseLogs(logs: any[]): Array<Log>;
    protected _prepareFilter(filter: EventFilter): {
        fromBlock: string;
        toBlock: string;
        topics?: (string | string[])[];
        address?: string | string[];
        blockHash?: string;
    };
    _wrapTransaction(tx: ethers.Transaction, hash?: string): TransactionResponse;
    getTransactionStatus(txHash: string): Promise<TransactionStatus>;
    getTransaction(hash: string | Promise<string>): Promise<TransactionResponse>;
    sendTransaction(transaction: string | Promise<string>): Promise<TransactionResponse>;
    getL2TransactionFromPriorityOp(l1TxResponse: ethers.providers.TransactionResponse): Promise<TransactionResponse>;
    getPriorityOpResponse(l1TxResponse: ethers.providers.TransactionResponse): Promise<PriorityOpResponse>;
    getContractAccountInfo(address: Address): Promise<ContractAccountInfo>;
    estimateL1ToL2Execute(transaction: {
        contractAddress: Address;
        calldata: BytesLike;
        caller?: Address;
        l2Value?: BigNumberish;
        factoryDeps?: ethers.BytesLike[];
        gasPerPubdataByte?: BigNumberish;
        overrides?: ethers.PayableOverrides;
    }): Promise<BigNumber>;
}
export declare class Web3Provider extends Provider {
    readonly provider: ExternalProvider;
    constructor(provider: ExternalProvider, network?: ethers.providers.Networkish);
    send(method: string, params?: Array<any>): Promise<any>;
    getSigner(addressOrIndex?: number | string): Signer;
    estimateGas(transaction: ethers.utils.Deferrable<TransactionRequest>): Promise<ethers.BigNumber>;
}

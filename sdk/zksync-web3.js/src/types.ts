import { BytesLike, BigNumberish, providers, BigNumber } from 'ethers';
import { BlockWithTransactions as EthersBlockWithTransactions } from '@ethersproject/abstract-provider';

// 0x-prefixed, hex encoded, ethereum account address
export type Address = string;
// 0x-prefixed, hex encoded, ECDSA signature.
export type Signature = string;

// Ethereum network
export enum Network {
    Mainnet = 1,
    Ropsten = 3,
    Rinkeby = 4,
    Goerli = 5,
    Localhost = 9
}

export enum PriorityQueueType {
    Deque = 0,
    HeapBuffer = 1,
    Heap = 2
}

export enum PriorityOpTree {
    Full = 0,
    Rollup = 1
}

export enum TransactionStatus {
    NotFound = 'not-found',
    Processing = 'processing',
    Committed = 'committed',
    Finalized = 'finalized'
}

export type PaymasterParams = {
    paymaster: Address;
    paymasterInput: BytesLike;
};

export type Eip712Meta = {
    gasPerPubdata?: BigNumberish;
    factoryDeps?: BytesLike[];
    customSignature?: BytesLike;
    paymasterParams?: PaymasterParams;
};

// prettier-ignore
export type BlockTag =
    | number
    | string // hex number
    | 'committed'
    | 'finalized'
    | 'latest'
    | 'earliest'
    | 'pending';

// TODO (SMA-1585): Support create2 variants.
export type DeploymentType = 'create' | 'createAccount';

export interface Token {
    l1Address: Address;
    l2Address: Address;
    /** @deprecated This field is here for backward compatibility - please use l2Address field instead */
    address: Address;
    name: string;
    symbol: string;
    decimals: number;
}

export interface MessageProof {
    id: number;
    proof: string[];
    root: string;
}

export interface EventFilter {
    topics?: Array<string | Array<string> | null>;
    address?: Address | Array<Address>;
    fromBlock?: BlockTag;
    toBlock?: BlockTag;
    blockHash?: string;
}

export interface TransactionResponse extends providers.TransactionResponse {
    l1BatchNumber: number;
    l1BatchTxIndex: number;
    waitFinalize(): Promise<TransactionReceipt>;
}

export interface TransactionReceipt extends providers.TransactionReceipt {
    l1BatchNumber: number;
    l1BatchTxIndex: number;
    logs: Array<Log>;
    l2ToL1Logs: Array<L2ToL1Log>;
}

export interface Block extends providers.Block {
    l1BatchNumber: number;
    l1BatchTimestamp: number;
}

export interface BlockWithTransactions extends EthersBlockWithTransactions {
    l1BatchNumber: number;
    l1BatchTimestamp: number;
    transactions: Array<TransactionResponse>;
}

export interface Log extends providers.Log {
    l1BatchNumber: number;
}

export interface L2ToL1Log {
    blockNumber: number;
    blockHash: string;
    l1BatchNumber: number;
    transactionIndex: number;
    txIndexInL1Batch?: number;
    shardId: number;
    isService: boolean;
    sender: string;
    key: string;
    value: string;
    transactionHash: string;
    logIndex: number;
}

export type TransactionRequest = providers.TransactionRequest & {
    customData?: Eip712Meta;
};

export interface PriorityOpResponse extends TransactionResponse {
    waitL1Commit(confirmation?: number): Promise<providers.TransactionReceipt>;
}

export type BalancesMap = { [key: string]: BigNumber };

export interface DeploymentInfo {
    sender: Address;
    bytecodeHash: string;
    deployedAddress: Address;
}

export interface ApprovalBasedPaymasterInput {
    type: 'ApprovalBased';
    token: Address;
    minimalAllowance: BigNumber;
    innerInput: BytesLike;
}

export interface GeneralPaymasterInput {
    type: 'General';
    innerInput: BytesLike;
}

export interface EthereumSignature {
    v: number;
    r: BytesLike;
    s: BytesLike;
}

export type PaymasterInput = ApprovalBasedPaymasterInput | GeneralPaymasterInput;

export enum AccountAbstractionVersion {
    None = 0,
    Version1 = 1
}

export enum AccountNonceOrdering {
    Sequential = 0,
    Arbitrary = 1
}

export interface ContractAccountInfo {
    supportedAAVersion: AccountAbstractionVersion;
    nonceOrdering: AccountNonceOrdering;
}

export interface BatchDetails {
    number: number;
    timestamp: number;
    l1TxCount: number;
    l2TxCount: number;
    rootHash?: string;
    status: string;
    commitTxHash?: string;
    committedAt?: Date;
    proveTxHash?: string;
    provenAt?: Date;
    executeTxHash?: string;
    executedAt?: Date;
    l1GasPrice: number;
    l2FairGasPrice: number;
}

export interface BlockDetails {
    number: number;
    timestamp: number;
    l1BatchNumber: number;
    l1TxCount: number;
    l2TxCount: number;
    rootHash?: string;
    status: string;
    commitTxHash?: string;
    committedAt?: Date;
    proveTxHash?: string;
    provenAt?: Date;
    executeTxHash?: string;
    executedAt?: Date;
}

export interface TransactionDetails {
    isL1Originated: boolean;
    status: string;
    fee: BigNumberish;
    gasPerPubdata?: BigNumberish;
    initiatorAddress: Address;
    receivedAt: Date;
    ethCommitTxHash?: string;
    ethProveTxHash?: string;
    ethExecuteTxHash?: string;
}

export interface FullDepositFee {
    maxFeePerGas?: BigNumber;
    maxPriorityFeePerGas?: BigNumber;
    gasPrice?: BigNumber;
    baseCost: BigNumber;
    l1GasLimit: BigNumber;
    l2GasLimit: BigNumber;
}

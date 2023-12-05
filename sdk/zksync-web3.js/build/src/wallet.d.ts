import { EIP712Signer } from './signer';
import { Provider } from './provider';
import { ethers, utils } from 'ethers';
import { BlockTag, TransactionResponse, TransactionRequest } from './types';
import { ProgressCallback } from '@ethersproject/json-wallets';
declare const Wallet_base: {
    new (...args: any[]): {
        _providerL2(): Provider;
        _signerL2(): ethers.Signer;
        getBalance(token?: string, blockTag?: BlockTag): Promise<ethers.BigNumber>;
        getAllBalances(): Promise<import("./types").BalancesMap>;
        getL2BridgeContracts(): Promise<{
            erc20: import("../typechain").IL2Bridge;
            weth: import("../typechain").IL2Bridge;
        }>;
        _fillCustomData(data: import("./types").Eip712Meta): import("./types").Eip712Meta;
        withdraw(transaction: {
            token: string;
            amount: ethers.BigNumberish;
            to?: string;
            bridgeAddress?: string;
            overrides?: ethers.Overrides;
        }): Promise<TransactionResponse>;
        transfer(transaction: {
            to: string;
            amount: ethers.BigNumberish;
            token?: string;
            overrides?: ethers.Overrides;
        }): Promise<TransactionResponse>;
        sendTransaction(tx: ethers.providers.TransactionRequest): Promise<ethers.providers.TransactionResponse>;
        getAddress(): Promise<string>;
    };
} & {
    new (...args: any[]): {
        _providerL2(): Provider;
        _providerL1(): ethers.providers.Provider;
        _signerL1(): ethers.Signer;
        getMainContract(): Promise<import("../typechain").IZkSync>;
        getL1BridgeContracts(): Promise<{
            erc20: import("../typechain").IL1Bridge;
            weth: import("../typechain").IL1Bridge;
        }>;
        getBalanceL1(token?: string, blockTag?: ethers.providers.BlockTag): Promise<ethers.BigNumber>;
        getAllowanceL1(token: string, bridgeAddress?: string, blockTag?: ethers.providers.BlockTag): Promise<ethers.BigNumber>;
        l2TokenAddress(token: string): Promise<string>;
        approveERC20(token: string, amount: ethers.BigNumberish, overrides?: ethers.Overrides & {
            bridgeAddress?: string;
        }): Promise<ethers.providers.TransactionResponse>;
        getBaseCost(params: {
            gasLimit: ethers.BigNumberish;
            gasPerPubdataByte?: ethers.BigNumberish;
            gasPrice?: ethers.BigNumberish;
        }): Promise<ethers.BigNumber>;
        deposit(transaction: {
            token: string;
            amount: ethers.BigNumberish;
            to?: string;
            operatorTip?: ethers.BigNumberish;
            bridgeAddress?: string;
            approveERC20?: boolean;
            l2GasLimit?: ethers.BigNumberish;
            gasPerPubdataByte?: ethers.BigNumberish;
            refundRecipient?: string;
            overrides?: ethers.PayableOverrides;
            approveOverrides?: ethers.Overrides;
            customBridgeData?: ethers.utils.BytesLike;
        }): Promise<import("./types").PriorityOpResponse>;
        estimateGasDeposit(transaction: {
            token: string;
            amount: ethers.BigNumberish;
            to?: string;
            operatorTip?: ethers.BigNumberish;
            bridgeAddress?: string;
            customBridgeData?: ethers.utils.BytesLike;
            l2GasLimit?: ethers.BigNumberish;
            gasPerPubdataByte?: ethers.BigNumberish;
            refundRecipient?: string;
            overrides?: ethers.PayableOverrides;
        }): Promise<ethers.BigNumber>;
        getDepositTx(transaction: {
            token: string;
            amount: ethers.BigNumberish;
            to?: string;
            operatorTip?: ethers.BigNumberish;
            bridgeAddress?: string;
            l2GasLimit?: ethers.BigNumberish;
            gasPerPubdataByte?: ethers.BigNumberish;
            customBridgeData?: ethers.utils.BytesLike;
            refundRecipient?: string;
            overrides?: ethers.PayableOverrides;
        }): Promise<any>;
        getFullRequiredDepositFee(transaction: {
            token: string;
            to?: string;
            bridgeAddress?: string;
            customBridgeData?: ethers.utils.BytesLike;
            gasPerPubdataByte?: ethers.BigNumberish;
            overrides?: ethers.PayableOverrides;
        }): Promise<import("./types").FullDepositFee>;
        _getWithdrawalLog(withdrawalHash: ethers.utils.BytesLike, index?: number): Promise<{
            log: import("./types").Log;
            l1BatchTxId: number;
        }>;
        _getWithdrawalL2ToL1Log(withdrawalHash: ethers.utils.BytesLike, index?: number): Promise<{
            l2ToL1LogIndex: number;
            l2ToL1Log: import("./types").L2ToL1Log;
        }>;
        finalizeWithdrawalParams(withdrawalHash: ethers.utils.BytesLike, index?: number): Promise<{
            l1BatchNumber: number;
            l2MessageIndex: number;
            l2TxNumberInBlock: number;
            message: any;
            sender: string;
            proof: string[];
        }>;
        finalizeWithdrawal(withdrawalHash: ethers.utils.BytesLike, index?: number, overrides?: ethers.Overrides): Promise<ethers.ContractTransaction>;
        isWithdrawalFinalized(withdrawalHash: ethers.utils.BytesLike, index?: number): Promise<boolean>;
        claimFailedDeposit(depositHash: ethers.utils.BytesLike, overrides?: ethers.Overrides): Promise<ethers.ContractTransaction>;
        requestExecute(transaction: {
            contractAddress: string;
            calldata: ethers.utils.BytesLike;
            l2GasLimit?: ethers.BigNumberish;
            l2Value?: ethers.BigNumberish;
            factoryDeps?: ethers.utils.BytesLike[];
            operatorTip?: ethers.BigNumberish;
            gasPerPubdataByte?: ethers.BigNumberish;
            refundRecipient?: string;
            overrides?: ethers.PayableOverrides;
        }): Promise<import("./types").PriorityOpResponse>;
        estimateGasRequestExecute(transaction: {
            contractAddress: string;
            calldata: ethers.utils.BytesLike;
            l2GasLimit?: ethers.BigNumberish;
            l2Value?: ethers.BigNumberish;
            factoryDeps?: ethers.utils.BytesLike[];
            operatorTip?: ethers.BigNumberish;
            gasPerPubdataByte?: ethers.BigNumberish;
            refundRecipient?: string;
            overrides?: ethers.PayableOverrides;
        }): Promise<ethers.BigNumber>;
        getRequestExecuteTx(transaction: {
            contractAddress: string;
            calldata: ethers.utils.BytesLike;
            l2GasLimit?: ethers.BigNumberish;
            l2Value?: ethers.BigNumberish;
            factoryDeps?: ethers.utils.BytesLike[];
            operatorTip?: ethers.BigNumberish;
            gasPerPubdataByte?: ethers.BigNumberish;
            refundRecipient?: string;
            overrides?: ethers.PayableOverrides;
        }): Promise<ethers.PopulatedTransaction>;
        sendTransaction(tx: ethers.providers.TransactionRequest): Promise<ethers.providers.TransactionResponse>;
        getAddress(): Promise<string>;
    };
} & typeof ethers.Wallet;
export declare class Wallet extends Wallet_base {
    readonly provider: Provider;
    providerL1?: ethers.providers.Provider;
    eip712: EIP712Signer;
    _providerL1(): ethers.providers.Provider;
    _providerL2(): Provider;
    _signerL1(): ethers.Wallet;
    _signerL2(): this;
    ethWallet(): ethers.Wallet;
    getNonce(blockTag?: BlockTag): Promise<number>;
    connect(provider: Provider): Wallet;
    connectToL1(provider: ethers.providers.Provider): Wallet;
    static fromMnemonic(mnemonic: string, path?: string, wordlist?: ethers.Wordlist): Wallet;
    static fromEncryptedJson(json: string, password?: string | ethers.Bytes, callback?: ProgressCallback): Promise<Wallet>;
    static fromEncryptedJsonSync(json: string, password?: string | ethers.Bytes): Wallet;
    static createRandom(options?: any): Wallet;
    constructor(privateKey: ethers.BytesLike | utils.SigningKey, providerL2?: Provider, providerL1?: ethers.providers.Provider);
    populateTransaction(transaction: TransactionRequest): Promise<TransactionRequest>;
    signTransaction(transaction: TransactionRequest): Promise<string>;
    sendTransaction(transaction: ethers.providers.TransactionRequest): Promise<TransactionResponse>;
}
export {};

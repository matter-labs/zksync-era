import { ethers } from 'ethers';
import { Provider } from './provider';
import { BlockTag, TransactionResponse, Signature, TransactionRequest } from './types';
import { TypedDataSigner } from '@ethersproject/abstract-signer';
export declare const eip712Types: {
    Transaction: {
        name: string;
        type: string;
    }[];
};
export declare class EIP712Signer {
    private ethSigner;
    private eip712Domain;
    constructor(ethSigner: ethers.Signer & TypedDataSigner, chainId: number | Promise<number>);
    static getSignInput(transaction: TransactionRequest): {
        txType: number;
        from: string;
        to: string;
        gasLimit: ethers.BigNumberish;
        gasPerPubdataByteLimit: ethers.BigNumberish;
        maxFeePerGas: ethers.BigNumberish;
        maxPriorityFeePerGas: ethers.BigNumberish;
        paymaster: string;
        nonce: ethers.BigNumberish;
        value: ethers.BigNumberish;
        data: ethers.utils.BytesLike;
        factoryDeps: Uint8Array[];
        paymasterInput: ethers.utils.BytesLike;
    };
    sign(transaction: TransactionRequest): Promise<Signature>;
    static getSignedDigest(transaction: TransactionRequest): ethers.BytesLike;
}
declare const Signer_base: {
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
} & typeof ethers.providers.JsonRpcSigner;
export declare class Signer extends Signer_base {
    provider: Provider;
    eip712: EIP712Signer;
    _signerL2(): this;
    _providerL2(): Provider;
    static from(signer: ethers.providers.JsonRpcSigner & {
        provider: Provider;
    }): Signer;
    getNonce(blockTag?: BlockTag): Promise<number>;
    sendTransaction(transaction: TransactionRequest): Promise<TransactionResponse>;
}
declare const L1Signer_base: {
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
} & typeof ethers.providers.JsonRpcSigner;
export declare class L1Signer extends L1Signer_base {
    providerL2: Provider;
    _providerL2(): Provider;
    _providerL1(): ethers.providers.JsonRpcProvider;
    _signerL1(): this;
    static from(signer: ethers.providers.JsonRpcSigner, zksyncProvider: Provider): L1Signer;
    connectToL2(provider: Provider): this;
}
declare const L2VoidSigner_base: {
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
} & typeof ethers.VoidSigner;
export declare class L2VoidSigner extends L2VoidSigner_base {
    provider: Provider;
    eip712: EIP712Signer;
    _signerL2(): this;
    _providerL2(): Provider;
    static from(signer: ethers.VoidSigner & {
        provider: Provider;
    }): L2VoidSigner;
    getNonce(blockTag?: BlockTag): Promise<number>;
    sendTransaction(transaction: TransactionRequest): Promise<TransactionResponse>;
}
declare const L1VoidSigner_base: {
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
} & typeof ethers.VoidSigner;
export declare class L1VoidSigner extends L1VoidSigner_base {
    providerL2: Provider;
    _providerL2(): Provider;
    _providerL1(): ethers.providers.Provider;
    _signerL1(): this;
    static from(signer: ethers.VoidSigner, zksyncProvider: Provider): L1VoidSigner;
    connectToL2(provider: Provider): this;
}
export {};

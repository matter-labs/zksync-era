import { BigNumber, BigNumberish, BytesLike, ethers } from 'ethers';
import { Provider } from './provider';
import { Address, BalancesMap, BlockTag, Eip712Meta, FullDepositFee, PriorityOpResponse, TransactionResponse } from './types';
declare type Constructor<T = {}> = new (...args: any[]) => T;
interface TxSender {
    sendTransaction(tx: ethers.providers.TransactionRequest): Promise<ethers.providers.TransactionResponse>;
    getAddress(): Promise<Address>;
}
export declare function AdapterL1<TBase extends Constructor<TxSender>>(Base: TBase): {
    new (...args: any[]): {
        _providerL2(): Provider;
        _providerL1(): ethers.providers.Provider;
        _signerL1(): ethers.Signer;
        getMainContract(): Promise<import("../typechain").IZkSync>;
        getL1BridgeContracts(): Promise<{
            erc20: import("../typechain").IL1Bridge;
            weth: import("../typechain").IL1Bridge;
        }>;
        getBalanceL1(token?: Address, blockTag?: ethers.providers.BlockTag): Promise<BigNumber>;
        getAllowanceL1(token: Address, bridgeAddress?: Address, blockTag?: ethers.providers.BlockTag): Promise<BigNumber>;
        l2TokenAddress(token: Address): Promise<string>;
        approveERC20(token: Address, amount: BigNumberish, overrides?: ethers.Overrides & {
            bridgeAddress?: Address;
        }): Promise<ethers.providers.TransactionResponse>;
        getBaseCost(params: {
            gasLimit: BigNumberish;
            gasPerPubdataByte?: BigNumberish;
            gasPrice?: BigNumberish;
        }): Promise<BigNumber>;
        deposit(transaction: {
            token: Address;
            amount: BigNumberish;
            to?: Address;
            operatorTip?: BigNumberish;
            bridgeAddress?: Address;
            approveERC20?: boolean;
            l2GasLimit?: BigNumberish;
            gasPerPubdataByte?: BigNumberish;
            refundRecipient?: Address;
            overrides?: ethers.PayableOverrides;
            approveOverrides?: ethers.Overrides;
            customBridgeData?: BytesLike;
        }): Promise<PriorityOpResponse>;
        estimateGasDeposit(transaction: {
            token: Address;
            amount: BigNumberish;
            to?: Address;
            operatorTip?: BigNumberish;
            bridgeAddress?: Address;
            customBridgeData?: BytesLike;
            l2GasLimit?: BigNumberish;
            gasPerPubdataByte?: BigNumberish;
            refundRecipient?: Address;
            overrides?: ethers.PayableOverrides;
        }): Promise<ethers.BigNumber>;
        getDepositTx(transaction: {
            token: Address;
            amount: BigNumberish;
            to?: Address;
            operatorTip?: BigNumberish;
            bridgeAddress?: Address;
            l2GasLimit?: BigNumberish;
            gasPerPubdataByte?: BigNumberish;
            customBridgeData?: BytesLike;
            refundRecipient?: Address;
            overrides?: ethers.PayableOverrides;
        }): Promise<any>;
        getFullRequiredDepositFee(transaction: {
            token: Address;
            to?: Address;
            bridgeAddress?: Address;
            customBridgeData?: BytesLike;
            gasPerPubdataByte?: BigNumberish;
            overrides?: ethers.PayableOverrides;
        }): Promise<FullDepositFee>;
        _getWithdrawalLog(withdrawalHash: BytesLike, index?: number): Promise<{
            log: import("./types").Log;
            l1BatchTxId: number;
        }>;
        _getWithdrawalL2ToL1Log(withdrawalHash: BytesLike, index?: number): Promise<{
            l2ToL1LogIndex: number;
            l2ToL1Log: import("./types").L2ToL1Log;
        }>;
        finalizeWithdrawalParams(withdrawalHash: BytesLike, index?: number): Promise<{
            l1BatchNumber: number;
            l2MessageIndex: number;
            l2TxNumberInBlock: number;
            message: any;
            sender: string;
            proof: string[];
        }>;
        finalizeWithdrawal(withdrawalHash: BytesLike, index?: number, overrides?: ethers.Overrides): Promise<ethers.ContractTransaction>;
        isWithdrawalFinalized(withdrawalHash: BytesLike, index?: number): Promise<boolean>;
        claimFailedDeposit(depositHash: BytesLike, overrides?: ethers.Overrides): Promise<ethers.ContractTransaction>;
        requestExecute(transaction: {
            contractAddress: Address;
            calldata: BytesLike;
            l2GasLimit?: BigNumberish;
            l2Value?: BigNumberish;
            factoryDeps?: ethers.BytesLike[];
            operatorTip?: BigNumberish;
            gasPerPubdataByte?: BigNumberish;
            refundRecipient?: Address;
            overrides?: ethers.PayableOverrides;
        }): Promise<PriorityOpResponse>;
        estimateGasRequestExecute(transaction: {
            contractAddress: Address;
            calldata: BytesLike;
            l2GasLimit?: BigNumberish;
            l2Value?: BigNumberish;
            factoryDeps?: ethers.BytesLike[];
            operatorTip?: BigNumberish;
            gasPerPubdataByte?: BigNumberish;
            refundRecipient?: Address;
            overrides?: ethers.PayableOverrides;
        }): Promise<ethers.BigNumber>;
        getRequestExecuteTx(transaction: {
            contractAddress: Address;
            calldata: BytesLike;
            l2GasLimit?: BigNumberish;
            l2Value?: BigNumberish;
            factoryDeps?: ethers.BytesLike[];
            operatorTip?: BigNumberish;
            gasPerPubdataByte?: BigNumberish;
            refundRecipient?: Address;
            overrides?: ethers.PayableOverrides;
        }): Promise<ethers.PopulatedTransaction>;
        sendTransaction(tx: ethers.providers.TransactionRequest): Promise<ethers.providers.TransactionResponse>;
        getAddress(): Promise<string>;
    };
} & TBase;
export declare function AdapterL2<TBase extends Constructor<TxSender>>(Base: TBase): {
    new (...args: any[]): {
        _providerL2(): Provider;
        _signerL2(): ethers.Signer;
        getBalance(token?: Address, blockTag?: BlockTag): Promise<BigNumber>;
        getAllBalances(): Promise<BalancesMap>;
        getL2BridgeContracts(): Promise<{
            erc20: import("../typechain").IL2Bridge;
            weth: import("../typechain").IL2Bridge;
        }>;
        _fillCustomData(data: Eip712Meta): Eip712Meta;
        withdraw(transaction: {
            token: Address;
            amount: BigNumberish;
            to?: Address;
            bridgeAddress?: Address;
            overrides?: ethers.Overrides;
        }): Promise<TransactionResponse>;
        transfer(transaction: {
            to: Address;
            amount: BigNumberish;
            token?: Address;
            overrides?: ethers.Overrides;
        }): Promise<TransactionResponse>;
        sendTransaction(tx: ethers.providers.TransactionRequest): Promise<ethers.providers.TransactionResponse>;
        getAddress(): Promise<string>;
    };
} & TBase;
export {};

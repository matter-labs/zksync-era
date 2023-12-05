import { Wallet } from './wallet';
import { Signer } from './signer';
import { Contract, ContractInterface, ethers } from 'ethers';
import { DeploymentType } from './types';
export { Contract } from 'ethers';
export declare class ContractFactory extends ethers.ContractFactory {
    readonly signer: Wallet | Signer;
    readonly deploymentType: DeploymentType;
    constructor(abi: ContractInterface, bytecode: ethers.BytesLike, signer: Wallet | Signer, deploymentType?: DeploymentType);
    private encodeCalldata;
    getDeployTransaction(...args: any[]): ethers.providers.TransactionRequest;
    deploy(...args: Array<any>): Promise<Contract>;
}

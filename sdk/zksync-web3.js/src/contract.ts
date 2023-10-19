import { Wallet } from './wallet';
import { Signer } from './signer';
import { BytesLike, Contract, ContractInterface, ethers, utils } from 'ethers';
import {
    hashBytecode,
    CONTRACT_DEPLOYER,
    CONTRACT_DEPLOYER_ADDRESS,
    EIP712_TX_TYPE,
    getDeployedContracts,
    DEFAULT_GAS_PER_PUBDATA_LIMIT
} from './utils';
import { AccountAbstractionVersion, DeploymentType } from './types';
export { Contract } from 'ethers';

export class ContractFactory extends ethers.ContractFactory {
    override readonly signer: Wallet | Signer;
    readonly deploymentType: DeploymentType;

    constructor(
        abi: ContractInterface,
        bytecode: ethers.BytesLike,
        signer: Wallet | Signer,
        deploymentType?: DeploymentType
    ) {
        super(abi, bytecode, signer);
        this.deploymentType = deploymentType || 'create';
    }

    private encodeCalldata(salt: BytesLike, bytecodeHash: BytesLike, constructorCalldata: BytesLike) {
        const deploymentTypeMap = {
            create: {
                functionName: 'create',
                additionalArgs: []
            },
            createAccount: {
                functionName: 'createAccount',
                additionalArgs: [AccountAbstractionVersion.Version1]
            }
        };
    
        const deploymentType = deploymentTypeMap[this.deploymentType];
    
        if (!deploymentType) {
            throw new Error(`Unsupported deployment type ${this.deploymentType}`);
        }
    
        const args = [salt, bytecodeHash, constructorCalldata, ...deploymentType.additionalArgs];
        return CONTRACT_DEPLOYER.encodeFunctionData(deploymentType.functionName, args);
    }

    override getDeployTransaction(...args: any[]): ethers.providers.TransactionRequest {
        // TODO (SMA-1585): Users should be able to provide the salt.
        const salt = '0x0000000000000000000000000000000000000000000000000000000000000000';

        // The overrides will be popped out in this call:
        const txRequest = super.getDeployTransaction(...args);
        // Removing overrides
        if (this.interface.deploy.inputs.length + 1 == args.length) {
            args.pop();
        }

        // Salt argument is not used, so we provide a placeholder value.
        const bytecodeHash = hashBytecode(this.bytecode);
        const constructorCalldata = utils.arrayify(this.interface.encodeDeploy(args));

        const deployCalldata = this.encodeCalldata(salt, bytecodeHash, constructorCalldata);

        return {
            ...txRequest,
            type: EIP712_TX_TYPE,
            to: CONTRACT_DEPLOYER_ADDRESS,
            data: deployCalldata,
            customData: {
                ...txRequest.customData,
                factoryDeps: [...(txRequest.customData?.factoryDeps || []), this.bytecode],
                gasPerPubdata: txRequest.customData?.gasPerPubdata || DEFAULT_GAS_PER_PUBDATA_LIMIT
            }
        };
    }

    override async deploy(...args: Array<any>): Promise<Contract> {
        const contract = await super.deploy(...args);

        const deployTxReceipt = await contract.deployTransaction.wait();

        const deployedAddresses = getDeployedContracts(deployTxReceipt).map((info) => info.deployedAddress);

        const contractWithCorrectAddress = new ethers.Contract(
            deployedAddresses[deployedAddresses.length - 1],
            contract.interface,
            contract.signer
        );
        utils.defineReadOnly(contractWithCorrectAddress, 'deployTransaction', contract.deployTransaction);
        return contractWithCorrectAddress;
    }
}

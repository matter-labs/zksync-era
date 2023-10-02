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
        if (this.deploymentType == 'create') {
            return CONTRACT_DEPLOYER.encodeFunctionData('create', [salt, bytecodeHash, constructorCalldata]);
        } else if (this.deploymentType == 'createAccount') {
            return CONTRACT_DEPLOYER.encodeFunctionData('createAccount', [
                salt,
                bytecodeHash,
                constructorCalldata,
                AccountAbstractionVersion.Version1
            ]);
        } else {
            throw new Error(`Unsupported deployment type ${this.deploymentType}`);
        }
    }

    override getDeployTransaction(...args: any[]): ethers.providers.TransactionRequest {
        // TODO (SMA-1585): Users should be able to provide the salt.
        let salt = '0x0000000000000000000000000000000000000000000000000000000000000000';

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

        txRequest.type = EIP712_TX_TYPE;
        txRequest.to = CONTRACT_DEPLOYER_ADDRESS;
        txRequest.data = deployCalldata;
        txRequest.customData ??= {};
        txRequest.customData.factoryDeps ??= [];
        txRequest.customData.gasPerPubdata ??= DEFAULT_GAS_PER_PUBDATA_LIMIT;
        // The number of factory deps is relatively low, so it is efficient enough.
        if (!txRequest.customData.factoryDeps.includes(this.bytecode)) {
            txRequest.customData.factoryDeps.push(this.bytecode);
        }

        return txRequest;
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

"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ContractFactory = exports.Contract = void 0;
const ethers_1 = require("ethers");
const utils_1 = require("./utils");
const types_1 = require("./types");
var ethers_2 = require("ethers");
Object.defineProperty(exports, "Contract", { enumerable: true, get: function () { return ethers_2.Contract; } });
class ContractFactory extends ethers_1.ethers.ContractFactory {
    constructor(abi, bytecode, signer, deploymentType) {
        super(abi, bytecode, signer);
        this.deploymentType = deploymentType || 'create';
    }
    encodeCalldata(salt, bytecodeHash, constructorCalldata) {
        if (this.deploymentType == 'create') {
            return utils_1.CONTRACT_DEPLOYER.encodeFunctionData('create', [salt, bytecodeHash, constructorCalldata]);
        }
        else if (this.deploymentType == 'createAccount') {
            return utils_1.CONTRACT_DEPLOYER.encodeFunctionData('createAccount', [
                salt,
                bytecodeHash,
                constructorCalldata,
                types_1.AccountAbstractionVersion.Version1
            ]);
        }
        else {
            throw new Error(`Unsupported deployment type ${this.deploymentType}`);
        }
    }
    getDeployTransaction(...args) {
        var _a, _b, _c;
        var _d, _e;
        // TODO (SMA-1585): Users should be able to provide the salt.
        let salt = '0x0000000000000000000000000000000000000000000000000000000000000000';
        // The overrides will be popped out in this call:
        const txRequest = super.getDeployTransaction(...args);
        // Removing overrides
        if (this.interface.deploy.inputs.length + 1 == args.length) {
            args.pop();
        }
        // Salt argument is not used, so we provide a placeholder value.
        const bytecodeHash = (0, utils_1.hashBytecode)(this.bytecode);
        const constructorCalldata = ethers_1.utils.arrayify(this.interface.encodeDeploy(args));
        const deployCalldata = this.encodeCalldata(salt, bytecodeHash, constructorCalldata);
        txRequest.type = utils_1.EIP712_TX_TYPE;
        txRequest.to = utils_1.CONTRACT_DEPLOYER_ADDRESS;
        txRequest.data = deployCalldata;
        (_a = txRequest.customData) !== null && _a !== void 0 ? _a : (txRequest.customData = {});
        (_b = (_d = txRequest.customData).factoryDeps) !== null && _b !== void 0 ? _b : (_d.factoryDeps = []);
        (_c = (_e = txRequest.customData).gasPerPubdata) !== null && _c !== void 0 ? _c : (_e.gasPerPubdata = utils_1.DEFAULT_GAS_PER_PUBDATA_LIMIT);
        // The number of factory deps is relatively low, so it is efficient enough.
        if (!txRequest.customData.factoryDeps.includes(this.bytecode)) {
            txRequest.customData.factoryDeps.push(this.bytecode);
        }
        return txRequest;
    }
    async deploy(...args) {
        const contract = await super.deploy(...args);
        const deployTxReceipt = await contract.deployTransaction.wait();
        const deployedAddresses = (0, utils_1.getDeployedContracts)(deployTxReceipt).map((info) => info.deployedAddress);
        const contractWithCorrectAddress = new ethers_1.ethers.Contract(deployedAddresses[deployedAddresses.length - 1], contract.interface, contract.signer);
        ethers_1.utils.defineReadOnly(contractWithCorrectAddress, 'deployTransaction', contract.deployTransaction);
        return contractWithCorrectAddress;
    }
}
exports.ContractFactory = ContractFactory;

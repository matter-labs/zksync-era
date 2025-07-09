import { BytesLike, ethers, BigNumberish } from 'ethers';
import { ForceDeployUpgraderFactory as ForceDeployUpgraderFactoryL2 } from 'l2-contracts/typechain';
import {
    DefaultUpgradeFactory as DefaultUpgradeFactoryL1,
    AdminFacetFactory,
    StateTransitionManagerFactory,
    ChainAdminFactory
} from 'l1-contracts/typechain';
import { FacetCut } from 'l1-contracts/src.ts/diamondCut';
import { ComplexUpgraderFactory } from 'system-contracts/typechain';
import {
    getCommonDataFileName,
    getCryptoFileName,
    getFacetCutsFileName,
    getL2TransactionsFileName,
    getPostUpgradeCalldataFileName,
    getL2UpgradeFileName,
    VerifierParams,
    unpackStringSemVer,
    packSemver
} from './utils';
import fs from 'fs';
import { Command } from 'commander';
import { web3Url } from 'utils';
import * as path from 'path';

const testConfigPath = path.join(process.env.ZKSYNC_HOME as string, `etc/test_config/constant`);
const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));

export enum Action {
    Add = 0,
    Replace = 1,
    Remove = 2
}

export interface DiamondCutData {
    facetCuts: FacetCut[];
    initAddress: string;
    initCalldata: string;
}

export interface ChainCreationParams {
    genesisUpgrade: string;
    genesisBatchHash: string;
    genesisIndexRepeatedStorageChanges: number;
    genesisBatchCommitment: string;
    diamondCut: DiamondCutData;
}

export interface ForceDeployment {
    // The bytecode hash to put on an address
    bytecodeHash: BytesLike;
    // The address on which to deploy the bytecodehash to
    newAddress: string;
    // Whether to call the constructor
    callConstructor: boolean;
    // The value with which to initialize a contract
    value: ethers.BigNumber;
    // The constructor calldata
    input: BytesLike;
}

export interface L2CanonicalTransaction {
    txType: BigNumberish;
    from: BigNumberish;
    to: BigNumberish;
    gasLimit: BigNumberish;
    gasPerPubdataByteLimit: BigNumberish;
    maxFeePerGas: BigNumberish;
    maxPriorityFeePerGas: BigNumberish;
    paymaster: BigNumberish;
    nonce: BigNumberish;
    value: BigNumberish;
    // In the future, we might want to add some
    // new fields to the struct. The `txData` struct
    // is to be passed to account and any changes to its structure
    // would mean a breaking change to these accounts. In order to prevent this,
    // we should keep some fields as "reserved".
    // It is also recommended that their length is fixed, since
    // it would allow easier proof integration (in case we will need
    // some special circuit for preprocessing transactions).
    reserved: [BigNumberish, BigNumberish, BigNumberish, BigNumberish];
    data: BytesLike;
    signature: BytesLike;
    factoryDeps: BigNumberish[];
    paymasterInput: BytesLike;
    // Reserved dynamic type for the future use-case. Using it should be avoided,
    // But it is still here, just in case we want to enable some additional functionality.
    reservedDynamic: BytesLike;
}

export interface ProposedUpgrade {
    // The tx for the upgrade call to the l2 system upgrade contract
    l2ProtocolUpgradeTx: L2CanonicalTransaction;
    factoryDeps: BytesLike[];
    bootloaderHash: BytesLike;
    defaultAccountHash: BytesLike;
    verifier: string;
    verifierParams: VerifierParams;
    l1ContractsUpgradeCalldata: BytesLike;
    postUpgradeCalldata: BytesLike;
    upgradeTimestamp: ethers.BigNumber;
    newProtocolVersion: BigNumberish;
}

function buildNoopL2UpgradeTx(): L2CanonicalTransaction {
    // L1 contract considers transaction with `txType` = 0 as noop.
    return {
        txType: 0,
        from: ethers.constants.AddressZero,
        to: ethers.constants.AddressZero,
        gasLimit: 0,
        gasPerPubdataByteLimit: 0,
        maxFeePerGas: 0,
        maxPriorityFeePerGas: 0,
        paymaster: 0,
        nonce: 0,
        value: 0,
        reserved: [0, 0, 0, 0],
        data: '0x',
        signature: '0x',
        factoryDeps: [],
        paymasterInput: '0x',
        reservedDynamic: '0x'
    };
}

export function buildProposeUpgrade(
    upgradeTimestamp: ethers.BigNumber,
    newProtocolVersion: number,
    l1ContractsUpgradeCalldata?: BytesLike,
    postUpgradeCalldata?: BytesLike,
    verifierParams?: VerifierParams,
    bootloaderHash?: BytesLike,
    defaultAccountHash?: BytesLike,
    verifier?: string,
    l2ProtocolUpgradeTx?: L2CanonicalTransaction
): ProposedUpgrade {
    bootloaderHash = bootloaderHash ?? ethers.constants.HashZero;
    defaultAccountHash = defaultAccountHash ?? ethers.constants.HashZero;
    l1ContractsUpgradeCalldata = l1ContractsUpgradeCalldata ?? '0x';
    postUpgradeCalldata = postUpgradeCalldata ?? '0x';
    l2ProtocolUpgradeTx = l2ProtocolUpgradeTx ?? buildNoopL2UpgradeTx();
    return {
        l2ProtocolUpgradeTx,
        bootloaderHash,
        defaultAccountHash,
        verifier,
        verifierParams,
        l1ContractsUpgradeCalldata,
        postUpgradeCalldata,
        upgradeTimestamp,
        factoryDeps: [],
        newProtocolVersion
    };
}

export function forceDeploymentCalldata(forcedDeployments: ForceDeployment[]): BytesLike {
    let forceDeployUpgrader = new ForceDeployUpgraderFactoryL2();
    let calldata = forceDeployUpgrader.interface.encodeFunctionData('forceDeploy', [forcedDeployments]);
    return calldata;
}

export function prepareCallDataForComplexUpgrader(calldata: BytesLike, to: string): BytesLike {
    const upgrader = new ComplexUpgraderFactory();
    let finalCalldata = upgrader.interface.encodeFunctionData('upgrade', [to, calldata]);
    return finalCalldata;
}

export function prepareDefaultCalldataForL1upgrade(upgrade: ProposedUpgrade): BytesLike {
    let defaultUpgrade = new DefaultUpgradeFactoryL1();
    let calldata = defaultUpgrade.interface.encodeFunctionData('upgrade', [upgrade]);
    return calldata;
}

export function prepareDefaultCalldataForL2upgrade(forcedDeployments: ForceDeployment[], l2UpgraderAddress): BytesLike {
    const forcedDeploymentsCalldata = forceDeploymentCalldata(forcedDeployments);
    const complexUpgraderCalldata = prepareCallDataForComplexUpgrader(forcedDeploymentsCalldata, l2UpgraderAddress);
    return complexUpgraderCalldata;
}

function prepareChainAdminCalldata(target: string, data: BytesLike): string {
    const call = {
        target: target,
        value: 0,
        data: data
    };

    const chainAdmin = new ChainAdminFactory();
    const calldata = chainAdmin.interface.encodeFunctionData('multicall', [[call], true]);

    return calldata;
}

export function prepareUpgradeCalldata(
    oldProtocolVersion,
    oldProtocolVersionDeadline,
    newProtocolVersion,
    initCalldata,
    upgradeAddress: string,
    facetCuts: FacetCut[],
    zksyncAddress: string,
    genesisUpgradeAddress: string,
    genesisBatchHash: string,
    genesisIndexRepeatedStorageChanges: number,
    genesisBatchCommitment: string,
    prepareDirectOperation?: boolean,
    chainId?: string
) {
    let diamondCut: DiamondCutData = {
        facetCuts,
        initAddress: upgradeAddress,
        initCalldata
    };

    let chainCreationDiamondCut: DiamondCutData = {
        facetCuts: facetCuts.filter((cut) => cut.action == Action.Add),
        initAddress: genesisUpgradeAddress,
        initCalldata: '0x'
    };

    let chainCreationParams: ChainCreationParams = {
        genesisUpgrade: genesisUpgradeAddress,
        genesisBatchHash,
        genesisIndexRepeatedStorageChanges,
        genesisBatchCommitment,
        diamondCut: chainCreationDiamondCut
    };

    // Prepare calldata for STM
    let stm = new StateTransitionManagerFactory();
    const stmUpgradeCalldata = stm.interface.encodeFunctionData('setNewVersionUpgrade', [
        diamondCut,
        oldProtocolVersion,
        oldProtocolVersionDeadline,
        newProtocolVersion
    ]);

    const stmSetChainCreationCalldata = stm.interface.encodeFunctionData('setChainCreationParams', [
        chainCreationParams
    ]);

    // Prepare calldata for upgrading diamond proxy
    let adminFacet = new AdminFacetFactory();
    const diamondProxyUpgradeCalldata = adminFacet.interface.encodeFunctionData('upgradeChainFromVersion', [
        oldProtocolVersion,
        diamondCut
    ]);

    const chainAdminUpgradeCalldata = prepareChainAdminCalldata(zksyncAddress, diamondProxyUpgradeCalldata);

    let result: any = {
        stmUpgradeCalldata,
        chainAdminUpgradeCalldata,
        diamondCut,
        stmSetChainCreationCalldata
    };

    if (prepareDirectOperation) {
        if (!chainId) {
            throw new Error('chainId is required for direct operation');
        }

        const stmDirecUpgradeCalldata = stm.interface.encodeFunctionData('executeUpgrade', [chainId, diamondCut]);

        result = {
            ...result,
            stmDirecUpgradeCalldata
        };
    }

    return result;
}

export function buildDefaultUpgradeTx(
    environment,
    upgradeAddress,
    oldProtocolVersion,
    oldProtocolVersionDeadline,
    upgradeTimestamp,
    zksyncAddress,
    postUpgradeCalldataFlag,
    genesisUpgradeAddress,
    genesisBatchHash,
    genesisIndexRepeatedStorageChanges,
    genesisBatchCommitment,
    prepareDirectOperation?,
    chainId?
) {
    const commonData = JSON.parse(fs.readFileSync(getCommonDataFileName(), { encoding: 'utf-8' }));
    const newProtocolVersionSemVer: string = commonData.protocolVersion;
    const packedNewProtocolVersion = packSemver(...unpackStringSemVer(newProtocolVersionSemVer));
    console.log(
        `Building default upgrade tx for ${environment} protocol version ${newProtocolVersionSemVer} upgradeTimestamp ${upgradeTimestamp} `
    );
    let facetCuts = [];
    let facetCutsFileName = getFacetCutsFileName(environment);
    if (fs.existsSync(facetCutsFileName)) {
        console.log(`Found facet cuts file ${facetCutsFileName}`);
        facetCuts = JSON.parse(fs.readFileSync(facetCutsFileName).toString());
    }
    upgradeAddress = upgradeAddress ?? process.env.CONTRACTS_DEFAULT_UPGRADE_ADDR;

    let bootloaderHash = ethers.constants.HashZero;
    let defaultAAHash = ethers.constants.HashZero;

    const l2upgradeFileName = getL2UpgradeFileName(environment);
    let l2UpgradeTx = undefined;
    if (fs.existsSync(l2upgradeFileName)) {
        console.log(`Found l2 upgrade file ${l2upgradeFileName}`);
        const l2Upgrade = JSON.parse(fs.readFileSync(l2upgradeFileName).toString());

        l2UpgradeTx = l2Upgrade.tx;
        if (l2Upgrade.bootloader) {
            bootloaderHash = l2Upgrade.bootloader.bytecodeHashes[0];
        }

        if (l2Upgrade.defaultAA) {
            defaultAAHash = l2Upgrade.defaultAA.bytecodeHashes[0];
        }
    }

    let cryptoVerifierAddress = ethers.constants.AddressZero;
    let cryptoVerifierParams = {
        recursionNodeLevelVkHash: ethers.constants.HashZero,
        recursionLeafLevelVkHash: ethers.constants.HashZero,
        recursionCircuitsSetVksHash: ethers.constants.HashZero
    };
    let cryptoFileName = getCryptoFileName(environment);
    if (fs.existsSync(cryptoFileName)) {
        console.log(`Found crypto file ${cryptoFileName}`);
        const crypto = JSON.parse(fs.readFileSync(cryptoFileName).toString());
        if (crypto.verifier) {
            cryptoVerifierAddress = crypto.verifier.address;
        }
        if (crypto.keys) {
            cryptoVerifierParams = crypto.keys;
        }
    }

    let postUpgradeCalldata = '0x';
    let postUpgradeCalldataFileName = getPostUpgradeCalldataFileName(environment);
    if (postUpgradeCalldataFlag) {
        if (fs.existsSync(postUpgradeCalldataFileName)) {
            console.log(`Found facet cuts file ${postUpgradeCalldataFileName}`);
            postUpgradeCalldata = JSON.parse(fs.readFileSync(postUpgradeCalldataFileName).toString());
        } else {
            throw new Error(`Post upgrade calldata file ${postUpgradeCalldataFileName} not found`);
        }
    }

    let proposeUpgradeTx = buildProposeUpgrade(
        ethers.BigNumber.from(upgradeTimestamp),
        packedNewProtocolVersion,
        '0x',
        postUpgradeCalldata,
        cryptoVerifierParams,
        bootloaderHash,
        defaultAAHash,
        cryptoVerifierAddress,
        l2UpgradeTx
    );

    let l1upgradeCalldata = prepareDefaultCalldataForL1upgrade(proposeUpgradeTx);

    let upgradeData = prepareUpgradeCalldata(
        oldProtocolVersion,
        oldProtocolVersionDeadline,
        packedNewProtocolVersion,
        l1upgradeCalldata,
        upgradeAddress,
        facetCuts,
        zksyncAddress,
        genesisUpgradeAddress,
        genesisBatchHash,
        genesisIndexRepeatedStorageChanges,
        genesisBatchCommitment,
        prepareDirectOperation,
        chainId
    );

    const transactions = {
        proposeUpgradeTx,
        l1upgradeCalldata,
        upgradeAddress,
        protocolVersionSemVer: newProtocolVersionSemVer,
        packedProtocolVersion: packedNewProtocolVersion,
        upgradeTimestamp,
        ...upgradeData
    };

    fs.writeFileSync(getL2TransactionsFileName(environment), JSON.stringify(transactions, null, 2));
    console.log('Default upgrade transactions are generated');
}

export function getWallet(l1rpc, privateKey) {
    if (!l1rpc) {
        l1rpc = web3Url();
    }
    const provider = new ethers.JsonRpcProvider(l1rpc);

    return privateKey
        ? new ethers.Wallet(privateKey, provider)
        : ethers.Wallet.fromMnemonic(
              process.env.MNEMONIC ? process.env.MNEMONIC : ethTestConfig.mnemonic,
              "m/44'/60'/0'/0/1"
          ).connect(provider);
}

export const command = new Command('transactions').description(
    'prepare the transactions and their calldata for the upgrade'
);

command
    .command('build-default')
    .requiredOption('--upgrade-timestamp <upgradeTimestamp>')
    .option('--upgrade-address <upgradeAddress>')
    .option('--environment <env>')
    .option('--old-protocol-version <oldProtocolVersion>')
    .option('--old-protocol-version-deadline <oldProtocolVersionDeadline>')
    .option('--l1rpc <l1prc>')
    .option('--zksync-address <zksyncAddress>')
    .option('--chain-id <chainId>')
    .option('--prepare-direct-operation <prepareDirectOperation>')
    .option('--post-upgrade-calldata <postUpgradeCalldata>')
    .option('--genesis-upgrade-address <genesisUpgradeAddress>')
    .option('--genesis-batch-hash <genesisBatchHash>')
    .option('--genesis-index-repeated-storage-changes <genesisIndexRepeatedStorageChanges>')
    .option('--genesis-batch-commitment <genesisBatchCommitment>')
    .action(async (options) => {
        buildDefaultUpgradeTx(
            options.environment,
            options.upgradeAddress,
            options.oldProtocolVersion,
            options.oldProtocolVersionDeadline,
            options.upgradeTimestamp,
            options.zksyncAddress,
            options.postUpgradeCalldata,
            options.genesisUpgradeAddress,
            options.genesisBatchHash,
            options.genesisIndexRepeatedStorageChanges,
            options.genesisBatchCommitment,
            options.prepareDirectOperation,
            options.chainId
        );
    });

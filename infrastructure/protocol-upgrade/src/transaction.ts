import { BigNumberish } from '@ethersproject/bignumber';
import { BytesLike, ethers } from 'ethers';
import { ForceDeployUpgraderFactory as ForceDeployUpgraderFactoryL2 } from 'l2-contracts/typechain';
import {
    DefaultUpgradeFactory as DefaultUpgradeFactoryL1,
    AdminFacetFactory,
    GovernanceFactory
} from 'l1-contracts/typechain';
import { FacetCut } from 'l1-contracts/src.ts/diamondCut';
import { IZkSyncFactory } from '../pre-boojum/IZkSyncFactory';
import { ComplexUpgraderFactory } from 'system-contracts/typechain';
import {
    getCommonDataFileName,
    getCryptoFileName,
    getFacetCutsFileName,
    getL2TransactionsFileName,
    getL2UpgradeFileName,
    VerifierParams
} from './utils';
import fs from 'fs';
import { Command } from 'commander';
import { web3Url } from 'zk/build/utils';
import * as path from 'path';

const testConfigPath = path.join(process.env.ZKSYNC_HOME as string, `etc/test_config/constant`);
const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));

export interface TransparentUpgrade {
    facetCuts: FacetCut[];
    initAddress: string;
    initCalldata: string;
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
    newAllowList: string;
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
    newAllowList?: string,
    l2ProtocolUpgradeTx?: L2CanonicalTransaction
): ProposedUpgrade {
    newAllowList = newAllowList ?? ethers.constants.AddressZero;
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
        newProtocolVersion,
        newAllowList
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

export function prepareProposeTransparentUpgradeCalldata(
    initCalldata,
    upgradeAddress: string,
    facetCuts: FacetCut[],
    diamondUpgradeProposalId: number
) {
    let zkSyncFactory = IZkSyncFactory.connect(upgradeAddress, ethers.providers.getDefaultProvider());
    let transparentUpgrade: TransparentUpgrade = {
        facetCuts,
        initAddress: upgradeAddress,
        initCalldata
    };

    let proposeTransparentUpgradeCalldata = zkSyncFactory.interface.encodeFunctionData('proposeTransparentUpgrade', [
        transparentUpgrade,
        diamondUpgradeProposalId
    ]);

    let executeUpgradeCalldata = zkSyncFactory.interface.encodeFunctionData('executeUpgrade', [
        transparentUpgrade,
        ethers.constants.HashZero
    ]);
    return {
        transparentUpgrade,
        proposeTransparentUpgradeCalldata,
        executeUpgradeCalldata
    };
}

export function prepareTransparentUpgradeCalldataForNewGovernance(
    initCalldata,
    upgradeAddress: string,
    facetCuts: FacetCut[],
    zksyncAddress: string
) {
    let transparentUpgrade: TransparentUpgrade = {
        facetCuts,
        initAddress: upgradeAddress,
        initCalldata
    };

    // Prepare calldata for upgrading diamond proxy
    let adminFacet = new AdminFacetFactory();
    const diamondProxyUpgradeCalldata = adminFacet.interface.encodeFunctionData('executeUpgrade', [transparentUpgrade]);

    const call = {
        target: zksyncAddress,
        value: 0,
        data: diamondProxyUpgradeCalldata
    };
    const governanceOperation = {
        calls: [call],
        predecessor: ethers.constants.HashZero,
        salt: ethers.constants.HashZero
    };

    const governance = new GovernanceFactory();
    // Get transaction data of the `scheduleTransparent`
    const scheduleTransparentOperation = governance.interface.encodeFunctionData('scheduleTransparent', [
        governanceOperation,
        0 // delay
    ]);

    // Get transaction data of the `execute`
    const executeOperation = governance.interface.encodeFunctionData('execute', [governanceOperation]);

    return {
        scheduleTransparentOperation,
        executeOperation,
        governanceOperation,
        transparentUpgrade
    };
}

export function buildDefaultUpgradeTx(
    environment,
    diamondUpgradeProposalId,
    upgradeAddress,
    l2UpgraderAddress,
    upgradeTimestamp,
    newAllowList,
    zksyncAddress,
    useNewGovernance
) {
    const commonData = JSON.parse(fs.readFileSync(getCommonDataFileName(), { encoding: 'utf-8' }));
    const protocolVersion = commonData.protocolVersion;
    console.log(
        `Building default upgrade tx for ${environment} protocol version ${protocolVersion} upgradeTimestamp ${upgradeTimestamp} `
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

    let proposeUpgradeTx = buildProposeUpgrade(
        ethers.BigNumber.from(upgradeTimestamp),
        protocolVersion,
        '0x',
        '0x',
        cryptoVerifierParams,
        bootloaderHash,
        defaultAAHash,
        cryptoVerifierAddress,
        newAllowList,
        l2UpgradeTx
    );

    let l1upgradeCalldata = prepareDefaultCalldataForL1upgrade(proposeUpgradeTx);

    let upgradeData;
    if (useNewGovernance) {
        upgradeData = prepareTransparentUpgradeCalldataForNewGovernance(
            l1upgradeCalldata,
            upgradeAddress,
            facetCuts,
            zksyncAddress
        );
    } else {
        upgradeData = prepareProposeTransparentUpgradeCalldata(
            l1upgradeCalldata,
            upgradeAddress,
            facetCuts,
            diamondUpgradeProposalId
        );
    }
    const transactions = {
        proposeUpgradeTx,
        l1upgradeCalldata,
        upgradeAddress,
        protocolVersion,
        diamondUpgradeProposalId,
        upgradeTimestamp,
        ...upgradeData
    };

    fs.writeFileSync(getL2TransactionsFileName(environment), JSON.stringify(transactions, null, 2));
    console.log('Default upgrade transactions are generated');
}

async function sendTransaction(
    calldata: BytesLike,
    privateKey: string,
    l1rpc: string,
    to: string,
    environment: string,
    gasPrice: ethers.BigNumber,
    nonce: number
) {
    const wallet = getWallet(l1rpc, privateKey);
    gasPrice = gasPrice ?? (await wallet.provider.getGasPrice());
    nonce = nonce ?? (await wallet.getTransactionCount());
    const tx = await wallet.sendTransaction({
        to,
        data: calldata,
        value: 0,
        gasLimit: 10_000_000,
        gasPrice,
        nonce
    });
    console.log('Transaction hash: ', tx.hash);
    await tx.wait();
    console.log('Transaction is executed');
}

function getWallet(l1rpc, privateKey) {
    if (!l1rpc) {
        l1rpc = web3Url();
    }
    const provider = new ethers.providers.JsonRpcProvider(l1rpc);

    return privateKey
        ? new ethers.Wallet(privateKey, provider)
        : ethers.Wallet.fromMnemonic(
              process.env.MNEMONIC ? process.env.MNEMONIC : ethTestConfig.mnemonic,
              "m/44'/60'/0'/0/1"
          ).connect(provider);
}

async function proposeUpgrade(
    privateKey: string,
    l1rpc: string,
    zksyncAddress: string,
    environment: string,
    gasPrice: ethers.BigNumber,
    nonce: number,
    newGovernanceAddress: string
) {
    const transactions = JSON.parse(fs.readFileSync(getL2TransactionsFileName(environment)).toString());
    let to;
    let calldata;
    if (newGovernanceAddress != null) {
        to = newGovernanceAddress;
        calldata = transactions.scheduleTransparentOperation;
    } else {
        to = zksyncAddress ?? process.env.CONTRACTS_DIAMOND_PROXY_ADDR;
        calldata = transactions.proposeTransparentUpgradeCalldata;
    }
    console.log(`Proposing upgrade for protocolVersion ${transactions.protocolVersion}`);
    await sendTransaction(calldata, privateKey, l1rpc, to, environment, gasPrice, nonce);
}

async function executeUpgrade(
    privateKey: string,
    l1rpc: string,
    zksyncAddress: string,
    environment: string,
    gasPrice: ethers.BigNumber,
    nonce: number,
    newGovernanceAddress: string
) {
    const transactions = JSON.parse(fs.readFileSync(getL2TransactionsFileName(environment)).toString());
    let to;
    let calldata;
    if (newGovernanceAddress != null) {
        to = newGovernanceAddress;
        calldata = transactions.executeOperation;
    } else {
        to = zksyncAddress ?? process.env.CONTRACTS_DIAMOND_PROXY_ADDR;
        calldata = transactions.executeUpgradeCalldata;
    }
    console.log(`Execute upgrade for protocolVersion ${transactions.protocolVersion}`);
    await sendTransaction(calldata, privateKey, l1rpc, to, environment, gasPrice, nonce);
}

async function cancelUpgrade(
    privateKey: string,
    l1rpc: string,
    zksyncAddress: string,
    environment: string,
    gasPrice: ethers.BigNumber,
    nonce: number,
    execute: boolean,
    newGovernanceAddress: string
) {
    if (newGovernanceAddress != null) {
        let wallet = getWallet(l1rpc, privateKey);
        const transactions = JSON.parse(fs.readFileSync(getL2TransactionsFileName(environment)).toString());

        let governance = GovernanceFactory.connect(newGovernanceAddress, wallet);
        const operation = transactions.governanceOperation;

        const operationId = await governance.hashOperation(operation);

        console.log(`Cancel upgrade operation with id: ${operationId}`);
        if (execute) {
            const tx = await governance.cancel(operationId);
            await tx.wait();
            console.log('Operation canceled');
        } else {
            const calldata = governance.interface.encodeFunctionData('cancel', [operationId]);
            console.log(`Cancel upgrade calldata: ${calldata}`);
        }
    } else {
        zksyncAddress = zksyncAddress ?? process.env.CONTRACTS_DIAMOND_PROXY_ADDR;
        let wallet = getWallet(l1rpc, privateKey);
        let zkSync = IZkSyncFactory.connect(zksyncAddress, wallet);
        const transactions = JSON.parse(fs.readFileSync(getL2TransactionsFileName(environment)).toString());

        const transparentUpgrade = transactions.transparentUpgrade;
        const diamondUpgradeProposalId = transactions.diamondUpgradeProposalId;

        const proposalHash = await zkSync.upgradeProposalHash(
            transparentUpgrade,
            diamondUpgradeProposalId,
            ethers.constants.HashZero
        );

        console.log(`Cancel upgrade with hash: ${proposalHash}`);
        let cancelUpgradeCalldata = zkSync.interface.encodeFunctionData('cancelUpgradeProposal', [proposalHash]);
        if (execute) {
            await sendTransaction(
                cancelUpgradeCalldata,
                privateKey,
                l1rpc,
                zksyncAddress,
                environment,
                gasPrice,
                nonce
            );
        } else {
            console.log(`Cancel upgrade calldata: ${cancelUpgradeCalldata}`);
        }
    }
}

async function getNewDiamondUpgradeProposalId(l1rpc: string, zksyncAddress: string) {
    zksyncAddress = zksyncAddress ?? process.env.CONTRACTS_DIAMOND_PROXY_ADDR;
    // We don't care about the wallet here, we just need to make a get call.
    let wallet = getWallet(l1rpc, undefined);
    let zkSync = IZkSyncFactory.connect(zksyncAddress, wallet);
    let proposalId = await zkSync.getCurrentProposalId();
    proposalId = proposalId.add(1);
    console.log(
        `New proposal id: ${proposalId} for ${zksyncAddress} network: ${JSON.stringify(
            await wallet.provider.getNetwork()
        )}`
    );
    return proposalId;
}

export const command = new Command('transactions').description(
    'prepare the transactions and their calldata for the upgrade'
);

command
    .command('build-default')
    .requiredOption('--upgrade-timestamp <upgradeTimestamp>')
    .option('--upgrade-address <upgradeAddress>')
    .option('--environment <env>')
    .option('--new-allow-list <newAllowList>')
    .option('--l2-upgrader-address <l2UpgraderAddress>')
    .option('--diamond-upgrade-proposal-id <diamondUpgradeProposalId>')
    .option('--l1rpc <l1prc>')
    .option('--zksync-address <zksyncAddress>')
    .option('--use-new-governance')
    .action(async (options) => {
        let diamondUpgradeProposalId = options.diamondUpgradeProposalId;
        if (!diamondUpgradeProposalId && !options.useNewGovernance) {
            diamondUpgradeProposalId = await getNewDiamondUpgradeProposalId(options.l1rpc, options.zksyncAddress);
        }

        buildDefaultUpgradeTx(
            options.environment,
            diamondUpgradeProposalId,
            options.upgradeAddress,
            options.l2UpgraderAddress,
            options.upgradeTimestamp,
            options.newAllowList,
            options.zksyncAddress,
            options.useNewGovernance
        );
    });

command
    .command('propose-upgrade')
    .option('--environment <env>')
    .option('--private-key <privateKey>')
    .option('--zksync-address <zksyncAddress>')
    .option('--gas-price <gasPrice>')
    .option('--nonce <nonce>')
    .option('--l1rpc <l1prc>')
    .option('--new-governance <newGovernance>')
    .action(async (options) => {
        await proposeUpgrade(
            options.privateKey,
            options.l1rpc,
            options.zksyncAddress,
            options.environment,
            options.gasPrice,
            options.nonce,
            options.newGovernance
        );
    });

command
    .command('execute-upgrade')
    .option('--environment <env>')
    .option('--private-key <privateKey>')
    .option('--zksync-address <zksyncAddress>')
    .option('--gas-price <gasPrice>')
    .option('--nonce <nonce>')
    .option('--l1rpc <l1prc>')
    .option('--new-governance <newGovernance>')
    .action(async (options) => {
        await executeUpgrade(
            options.privateKey,
            options.l1rpc,
            options.zksyncAddress,
            options.environment,
            options.gasPrice,
            options.nonce,
            options.newGovernance
        );
    });

command
    .command('cancel-upgrade')
    .option('--environment <env>')
    .option('--private-key <privateKey>')
    .option('--zksync-address <zksyncAddress>')
    .option('--gas-price <gasPrice>')
    .option('--nonce <nonce>')
    .option('--l1rpc <l1prc>')
    .option('--execute')
    .option('--new-governance <newGovernance>')
    .action(async (options) => {
        await cancelUpgrade(
            options.privateKey,
            options.l1rpc,
            options.zksyncAddress,
            options.environment,
            options.gasPrice,
            options.nonce,
            options.execute,
            options.newGovernance
        );
    });

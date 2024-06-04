import { BytesLike } from 'ethers';
import { ComplexUpgraderFactory, ContractDeployerFactory } from 'system-contracts/typechain';
import { ForceDeployment, L2CanonicalTransaction } from '../transaction';
import { ForceDeployUpgraderFactory } from 'l2-contracts/typechain';
import { Command } from 'commander';
import { getCommonDataFileName, getL2UpgradeFileName, unpackStringSemVer } from '../utils';
import fs from 'fs';
import { REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT } from 'zksync-ethers/build/utils';

const SYSTEM_UPGRADE_TX_TYPE = 254;
const FORCE_DEPLOYER_ADDRESS = '0x0000000000000000000000000000000000008007';
const CONTRACT_DEPLOYER_ADDRESS = '0x0000000000000000000000000000000000008006';
const COMPLEX_UPGRADE_ADDRESS = '0x000000000000000000000000000000000000800f';

function buildL2CanonicalTransaction(calldata: BytesLike, nonce, toAddress: string): L2CanonicalTransaction {
    return {
        txType: SYSTEM_UPGRADE_TX_TYPE,
        from: FORCE_DEPLOYER_ADDRESS,
        to: toAddress,
        gasLimit: 72_000_000,
        gasPerPubdataByteLimit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT,
        maxFeePerGas: 0,
        maxPriorityFeePerGas: 0,
        paymaster: 0,
        nonce,
        value: 0,
        reserved: [0, 0, 0, 0],
        data: calldata,
        signature: '0x',
        factoryDeps: [],
        paymasterInput: '0x',
        reservedDynamic: '0x'
    };
}

export function forceDeploymentCalldataUpgrader(forcedDeployments: ForceDeployment[]): BytesLike {
    let forceDeployUpgrader = new ForceDeployUpgraderFactory();
    let calldata = forceDeployUpgrader.interface.encodeFunctionData('forceDeploy', [forcedDeployments]);
    return calldata;
}

export function forceDeploymentCalldataContractDeployer(forcedDeployments: ForceDeployment[]): BytesLike {
    let contractDeployer = new ContractDeployerFactory();
    let calldata = contractDeployer.interface.encodeFunctionData('forceDeployOnAddresses', [forcedDeployments]);
    return calldata;
}

export function prepareCallDataForComplexUpgrader(calldata: BytesLike, to: string): BytesLike {
    const upgrader = new ComplexUpgraderFactory();
    let finalCalldata = upgrader.interface.encodeFunctionData('upgrade', [to, calldata]);
    return finalCalldata;
}

export const command = new Command('l2-transaction').description('publish system contracts');

command
    .command('force-deployment-calldata')
    .option('--environment <environment>')
    .option('--system-contracts-with-constructor', 'Call constructor to the list of the provided system contracts')
    .action(async (cmd) => {
        const systemContractsWithConstructor = cmd.systemContractsWithConstructor
            ? (cmd.systemContractsWithConstructor.split(',') as string[])
            : [];
        const l2upgradeFileName = getL2UpgradeFileName(cmd.environment);
        if (fs.existsSync(l2upgradeFileName)) {
            console.log(`Found l2 upgrade file ${l2upgradeFileName}`);
            let l2Upgrade = JSON.parse(fs.readFileSync(l2upgradeFileName).toString());
            const forcedDeployments = systemContractsToForceDeployments(
                l2Upgrade.systemContracts,
                systemContractsWithConstructor
            );
            const calldata = forceDeploymentCalldataContractDeployer(forcedDeployments);
            l2Upgrade.forcedDeployments = forcedDeployments;
            l2Upgrade.forcedDeploymentCalldata = calldata;
            l2Upgrade.delegatedCalldata = calldata;
            fs.writeFileSync(l2upgradeFileName, JSON.stringify(l2Upgrade, null, 2));
        } else {
            throw new Error(`No l2 upgrade file found at ${l2upgradeFileName}`);
        }
    });

function systemContractsToForceDeployments(
    systemContracts,
    systemContractsWithConstructor: string[]
): ForceDeployment[] {
    const forcedDeployments: ForceDeployment[] = systemContracts.map((dependency) => {
        return {
            bytecodeHash: dependency.bytecodeHashes[0],
            newAddress: dependency.address,
            value: 0,
            input: '0x',
            callConstructor: false
        };
    });

    for (const contractAddress of systemContractsWithConstructor) {
        const deploymentInfo = forcedDeployments.find((contract) => contract.newAddress === contractAddress);
        if (deploymentInfo) {
            deploymentInfo.callConstructor = true;
        } else {
            throw new Error(`Contract ${contractAddress} not found in forced deployments`);
        }
    }

    return forcedDeployments;
}

command
    .command('complex-upgrader-calldata')
    .option('--environment <environment>')
    .option('--l2-upgrader-address <l2UpgraderAddress>')
    .option(
        '--use-forced-deployments',
        'Build calldata with forced deployments instead of using prebuild delegated calldata'
    )
    .option(
        '--use-contract-deployer',
        'Use contract deployer address instead of complex upgrader address. ' +
            "Warning: this shouldn't be a default option, it's only for first upgrade purposes"
    )
    .option(
        '--system-contracts-with-constructor <systemContractsWithConstructor>',
        'Call constructor to the list of the provided system contracts'
    )
    .action(async (cmd) => {
        const l2upgradeFileName = getL2UpgradeFileName(cmd.environment);
        const l2UpgraderAddress = cmd.l2UpgraderAddress ?? process.env.CONTRACTS_L2_DEFAULT_UPGRADE_ADDR;
        const systemContractsWithConstructor = cmd.systemContractsWithConstructor
            ? (cmd.systemContractsWithConstructor.split(',') as string[])
            : [];
        const commonData = JSON.parse(fs.readFileSync(getCommonDataFileName(), { encoding: 'utf-8' }));
        if (fs.existsSync(l2upgradeFileName)) {
            console.log(`Found l2 upgrade file ${l2upgradeFileName}`);
            let l2Upgrade = JSON.parse(fs.readFileSync(l2upgradeFileName).toString());
            let delegatedCalldata = l2Upgrade.delegatedCalldata;
            if (cmd.useForcedDeployments) {
                l2Upgrade.forcedDeployments = systemContractsToForceDeployments(
                    l2Upgrade.systemContracts,
                    systemContractsWithConstructor
                );
                l2Upgrade.forcedDeploymentCalldata = forceDeploymentCalldataContractDeployer(
                    l2Upgrade.forcedDeployments
                );
                delegatedCalldata = l2Upgrade.forcedDeploymentCalldata;
            }
            let toAddress = COMPLEX_UPGRADE_ADDRESS;
            if (cmd.useContractDeployer) {
                toAddress = CONTRACT_DEPLOYER_ADDRESS;
                l2Upgrade.calldata = delegatedCalldata;
            } else {
                l2Upgrade.calldata = prepareCallDataForComplexUpgrader(delegatedCalldata, l2UpgraderAddress);
            }

            const protocolVersionSemVer: string = commonData.protocolVersion;
            const minorVersion = unpackStringSemVer(protocolVersionSemVer)[1];
            l2Upgrade.tx = buildL2CanonicalTransaction(l2Upgrade.calldata, minorVersion, toAddress);
            fs.writeFileSync(l2upgradeFileName, JSON.stringify(l2Upgrade, null, 2));
        } else {
            throw new Error(`No l2 upgrade file found at ${l2upgradeFileName}`);
        }
    });

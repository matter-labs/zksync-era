import fs from 'fs';
import { Command } from 'commander';
import { getL2UpgradeFileName, getUpgradePath } from 'utils';
import { callSystemContractDeployer } from './deployer';

async function publishAllFactoryDeps(
    l1RpcProvider: string,
    privateKey: string,
    l2RpcProvider: string,
    gasPrice: string,
    nonce: string,
    environment: string
) {
    console.log('Publishing bytecodes for all system contracts');
    const upgradeFile = getL2UpgradeFileName(environment);
    await callSystemContractDeployer(
        l1RpcProvider,
        privateKey,
        l2RpcProvider,
        gasPrice,
        nonce,
        true,
        true,
        true,
        upgradeFile
    );
}

async function publishAndMergeFiles(
    l1RpcProvider: string,
    privateKey: string,
    l2RpcProvider: string,
    gasPrice: string,
    nonce: string,
    bootloader: boolean,
    defaultAA: boolean,
    systemContracts: boolean,
    environment: string
) {
    console.log('Publishing bytecodes for system contracts');
    const upgradePath = getUpgradePath(environment);
    const tmpUpgradeFile = upgradePath + '/tmp.json';
    await callSystemContractDeployer(
        l1RpcProvider,
        privateKey,
        l2RpcProvider,
        gasPrice,
        nonce,
        bootloader,
        defaultAA,
        systemContracts,
        tmpUpgradeFile
    );
    const mainUpgradeFile = getL2UpgradeFileName(environment);
    let tmpUpgradeData = JSON.parse(fs.readFileSync(tmpUpgradeFile, 'utf8'));

    if (!fs.existsSync(mainUpgradeFile)) {
        fs.writeFileSync(mainUpgradeFile, JSON.stringify(tmpUpgradeData, null, 2));
        fs.unlinkSync(tmpUpgradeFile);
        return;
    }

    let mainUpgradeData = JSON.parse(fs.readFileSync(mainUpgradeFile, 'utf8'));
    if (bootloader !== undefined) {
        mainUpgradeData.bootloader = tmpUpgradeData.bootloader;
    }
    if (defaultAA !== undefined) {
        mainUpgradeData.defaultAA = tmpUpgradeData.defaultAA;
    }
    if (systemContracts) {
        mainUpgradeData.systemContracts = tmpUpgradeData.systemContracts;
    }
    fs.writeFileSync(mainUpgradeFile, JSON.stringify(mainUpgradeData, null, 2));
    fs.unlinkSync(tmpUpgradeFile);
    console.log('All system contracts published');
}

export const command = new Command('system-contracts').description('publish system contracts');

command
    .command('publish-all')
    .description('Publish all factory dependencies and base system contracts')
    .option('--private-key <private-key>')
    .option('--gas-price <gas-price>')
    .option('--nonce <nonce>')
    .option('--l1rpc <l1Rpc>')
    .option('--l2rpc <l2Rpc>')
    .option('--environment <environment>')
    .action(async (cmd) => {
        await publishAllFactoryDeps(cmd.l1rpc, cmd.privateKey, cmd.l2rpc, cmd.gasPrice, cmd.nonce, cmd.environment);
    });

command
    .command('publish')
    .description('Publish contracts one by one')
    .option('--private-key <private-key>')
    .option('--gas-price <gas-price>')
    .option('--nonce <nonce>')
    .option('--l1rpc <l1Rpc>')
    .option('--l2rpc <l2Rpc>')
    .option('--environment <environment>')
    .option('--bootloader')
    .option('--default-aa')
    .option('--system-contracts')
    .action(async (cmd) => {
        await publishAndMergeFiles(
            cmd.l1rpc,
            cmd.privateKey,
            cmd.l2rpc,
            cmd.gasPrice,
            cmd.nonce,
            cmd.bootloader,
            cmd.defaultAA,
            cmd.systemContracts,
            cmd.environment
        );
    });

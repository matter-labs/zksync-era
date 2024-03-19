import { Command } from 'commander';
import { prompt } from 'enquirer';
import chalk from 'chalk';
import { compileConfig } from './config';
import fs from 'fs';
import path from 'path';
import { set as setEnv } from './env';
import { setup as setupDb } from './database';
import * as utils from './utils';

enum Environment {
    Mainnet = 'mainnet',
    Testnet = 'testnet'
}

enum DataRetentionDuration {
    Hour = 'hour',
    Day = 'day',
    Week = 'week',
    Month = 'month',
    Year = 'year',
    Forever = 'forever'
}

async function selectDataRetentionDurationHours(): Promise<number | null> {
    const question = {
        type: 'select',
        name: 'retention',
        message: 'Select how long do you want to keep newest transactions data',
        choices: [
            { name: DataRetentionDuration.Hour, message: 'Hour', value: 1 },
            { name: DataRetentionDuration.Day, message: 'Day', value: 24 },
            { name: DataRetentionDuration.Week, message: 'Week', value: 24 * 7 },
            { name: DataRetentionDuration.Month, message: 'Month', value: 24 * 31 },
            { name: DataRetentionDuration.Year, message: 'Year', value: 24 * 366 },
            { name: DataRetentionDuration.Forever, message: 'Forever', value: null }
        ]
    };

    const answer: { retention: DataRetentionDuration } = await prompt(question);
    const choice = question.choices.find((choice) => choice.name === answer.retention);
    return choice ? choice.value : null;
}

async function selectEnvironment(): Promise<Environment> {
    const question = {
        type: 'select',
        name: 'environment',
        message: 'Select the environment:',
        choices: [
            { name: Environment.Testnet, message: 'Testnet (Sepolia)' },
            { name: Environment.Mainnet, message: 'Mainnet' }
        ]
    };

    const answer: { environment: Environment } = await prompt(question);
    return answer.environment;
}

async function removeConfigKey(env: string, key: string) {
    const filePath = path.join(path.join(process.env.ZKSYNC_HOME as string, `etc/env/${env}.toml`));
    const contents = await fs.promises.readFile(filePath, { encoding: 'utf-8' });

    const modifiedContents = contents
        .split('\n')
        .filter((line) => !line.startsWith(`${key} =`) && !line.startsWith(`${key}=`))
        .join('\n');
    await fs.promises.writeFile(filePath, modifiedContents);
}

async function changeConfigKey(env: string, key: string, newValue: string | number | boolean, section: string) {
    const filePath = path.join(path.join(process.env.ZKSYNC_HOME as string, `etc/env/${env}.toml`));
    let contents = await fs.promises.readFile(filePath, { encoding: 'utf-8' });

    const keyExists =
        contents.split('\n').find((line) => line.startsWith(`${key} =`) || line.startsWith(`${key}=`)) !== undefined;

    if (!keyExists) {
        contents = contents.replace(`\n[${section}]\n`, `\n[${section}]\n${key} =\n`);
    }

    const modifiedContents = contents
        .split('\n')
        .map((line) => (line.startsWith(`${key} =`) ? `${key} = ${JSON.stringify(newValue)}` : line))
        .map((line) => (line.startsWith(`${key}=`) ? `${key}=${JSON.stringify(newValue)}` : line))
        .join('\n');
    await fs.promises.writeFile(filePath, modifiedContents);
}

async function clearIfNeeded() {
    const filePath = path.join(path.join(process.env.ZKSYNC_HOME as string, `etc/env/ext-node.env`));
    if (!fs.existsSync(filePath)) {
        return true;
    }

    const question = {
        type: 'confirm',
        name: 'cleanup',
        message:
            'The external node files need to be cleared first, this will clear all its databases, do you want to continue?'
    };

    const answer: { cleanup: boolean } = await prompt(question);
    if (!answer.cleanup) {
        return false;
    }
    const cmd = chalk.yellow;
    console.log(`cleaning up database (${cmd('zk clean --config ext-node --database')})`);
    await utils.exec('zk clean --config ext-node --database');
    console.log(`cleaning up db (${cmd('zk db drop')})`);
    await utils.exec('zk db drop');
    return true;
}

async function runEnIfAskedTo() {
    const question = {
        type: 'confirm',
        name: 'runRequested',
        message: 'Do you want to run external-node now?'
    };
    const answer: { runRequested: boolean } = await prompt(question);
    if (!answer.runRequested) {
        return false;
    }
    await utils.spawn('zk external-node');
}

async function commentOutConfigKey(env: string, key: string) {
    const filePath = path.join(path.join(process.env.ZKSYNC_HOME as string, `etc/env/${env}.toml`));
    const contents = await fs.promises.readFile(filePath, { encoding: 'utf-8' });
    const modifiedContents = contents
        .split('\n')
        .map((line) => (line.startsWith(`${key} =`) || line.startsWith(`${key}=`) ? `#${line}` : line))
        .join('\n');
    await fs.promises.writeFile(filePath, modifiedContents);
}

async function configExternalNode() {
    const cmd = chalk.yellow;
    const success = chalk.green;
    const failure = chalk.red;

    console.log(`Changing active env to ext-node (${cmd('zk env ext-node')})`);
    setEnv('ext-node');

    const cleaningSucceeded = await clearIfNeeded();
    if (!cleaningSucceeded) {
        console.log(failure('Cleanup not allowed, but needed to proceed, exiting!'));
        return;
    }
    const env = await selectEnvironment();

    const retention = await selectDataRetentionDurationHours();
    await commentOutConfigKey('ext-node', 'template_database_url');
    await changeConfigKey('ext-node', 'mode', 'GCSAnonymousReadOnly', 'en.snapshots.object_store');
    await changeConfigKey('ext-node', 'snapshots_recovery_enabled', true, 'en');
    if (retention !== null) {
        await changeConfigKey('ext-node', 'pruning_data_retention_hours', retention, 'en');
    } else {
        await removeConfigKey('ext-node', 'pruning_data_retention_hours');
    }

    switch (env) {
        case Environment.Mainnet:
            await changeConfigKey('ext-node', 'l1_chain_id', 1, 'en');
            await changeConfigKey('ext-node', 'l2_chain_id', 324, 'en');
            await changeConfigKey('ext-node', 'main_node_url', 'https://mainnet.era.zksync.io', 'en');
            await changeConfigKey('ext-node', 'eth_client_url', 'https://ethereum-rpc.publicnode.com', 'en');
            await changeConfigKey(
                'ext-node',
                'bucket_base_url',
                'zksync-era-mainnet-external-node-snapshots',
                'en.snapshots.object_store'
            );
            break;
        case Environment.Testnet:
            await changeConfigKey('ext-node', 'l1_chain_id', 11155111, 'en');
            await changeConfigKey('ext-node', 'l2_chain_id', 300, 'en');
            await changeConfigKey('ext-node', 'main_node_url', 'https://sepolia.era.zksync.dev', 'en');
            await changeConfigKey('ext-node', 'eth_client_url', 'https://ethereum-sepolia-rpc.publicnode.com', 'en');
            await changeConfigKey(
                'ext-node',
                'bucket_base_url',
                'zksync-era-boojnet-external-node-snapshots',
                'en.snapshots.object_store'
            );
            break;
    }
    await compileConfig('ext-node');
    setEnv('ext-node');
    console.log(`Setting up postgres (${cmd('zk db setup')})`);
    await setupDb({ prover: false, server: true });

    console.log(`${success('Everything done!')} You can now run your external node using ${cmd('zk external-node')}`);
    await runEnIfAskedTo();
}

export const command = new Command('setup-external-node')
    .description('prepare local setup for running external-node on mainnet/testnet')
    .action(async (cmd: Command) => {
        await configExternalNode();
    });

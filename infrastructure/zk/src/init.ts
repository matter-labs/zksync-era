import { Command } from 'commander';
import chalk from 'chalk';
import * as utils from './utils';

import * as server from './server';
import * as contract from './contract';
import * as run from './run/run';
import * as compiler from './compiler';
import * as db from './database';
import { clean } from './clean';
import * as env from './env';
import * as docker from './docker';
import { up } from './up';

const entry = chalk.bold.yellow;
const announce = chalk.yellow;
const success = chalk.green;
const timestamp = chalk.grey;

export async function init(skipSubmodulesCheckout: boolean) {
    if (!process.env.CI) {
        await announced('Pulling images', docker.pull());
        await announced('Checking environment', checkEnv());
        await announced('Checking git hooks', env.gitHooks());
        await announced('Setting up containers', up());
        await announced('Checking PLONK setup', run.plonkSetup());
    }
    if (!skipSubmodulesCheckout) {
        await announced('Checkout system-contracts submodule', submoduleUpdate());
    }
    await announced('Compiling JS packages', run.yarn());
    await announced('Compile l2 contracts', compiler.compileAll());
    await announced('Drop postgres db', db.drop());
    await announced('Setup postgres db', db.setup());
    await announced('Clean rocksdb', clean('db'));
    await announced('Clean backups', clean('backups'));
    await announced('Building contracts', contract.build());
    await announced('Deploying localhost ERC20 tokens', run.deployERC20('dev'));
    await announced('Deploying L1 verifier', contract.deployVerifier([]));
    await announced('Reloading env', env.reload());
    await announced('Running server genesis setup', server.genesisFromSources());
    await announced('Deploying L1 contracts', contract.redeployL1([]));
    await announced('Initializing validator', contract.initializeValidator());
    await announced('Initialize L1 allow list', contract.initializeL1AllowList());
    await announced('Deploying L2 contracts', contract.deployL2([], true));
    await announced('Initializing L2 WETH token', contract.initializeWethToken());
}

// A smaller version of `init` that "resets" the localhost environment, for which `init` was already called before.
// It does less and runs much faster.
export async function reinit() {
    await announced('Setting up containers', up());
    await announced('Compiling JS packages', run.yarn());
    await announced('Compile l2 contracts', compiler.compileAll());
    await announced('Drop postgres db', db.drop());
    await announced('Setup postgres db', db.setup());
    await announced('Clean rocksdb', clean('db'));
    await announced('Clean backups', clean('backups'));
    await announced('Building contracts', contract.build());
    await announced('Deploying L1 verifier', contract.deployVerifier([]));
    await announced('Reloading env', env.reload());
    await announced('Running server genesis setup', server.genesisFromSources());
    await announced('Deploying L1 contracts', contract.redeployL1([]));
    await announced('Initializing validator', contract.initializeValidator());
    await announced('Initializing L1 Allow list', contract.initializeL1AllowList());
    await announced('Deploying L2 contracts', contract.deployL2([], true));
    await announced('Initializing L2 WETH token', contract.initializeWethToken());
}

// A lightweight version of `init` that sets up local databases, generates genesis and deploys precompiled contracts
export async function lightweightInit() {
    await announced('Clean rocksdb', clean('db'));
    await announced('Clean backups', clean('backups'));
    await announced('Deploying L1 verifier', contract.deployVerifier([]));
    await announced('Reloading env', env.reload());
    await announced('Running server genesis setup', server.genesisFromBinary());
    await announced('Deploying L1 contracts', contract.redeployL1([]));
    await announced('Initializing validator', contract.initializeValidator());
    await announced('Initializing L1 Allow list', contract.initializeL1AllowList());
    await announced('Deploying L2 contracts', contract.deployL2([], true));
    await announced('Initializing L2 WETH token', contract.initializeWethToken());
}

// Wrapper that writes an announcement and completion notes for each executed task.
export async function announced(fn: string, promise: Promise<void> | void) {
    const announceLine = `${entry('>')} ${announce(fn)}`;
    const separator = '-'.repeat(fn.length + 2); // 2 is the length of "> ".
    console.log(`\n` + separator); // So it's easier to see each individual step in the console.
    console.log(announceLine);

    const start = new Date().getTime();
    // The actual execution part
    await promise;

    const time = new Date().getTime() - start;
    const successLine = `${success('✔')} ${fn} done`;
    const timestampLine = timestamp(`(${time}ms)`);
    console.log(`${successLine} ${timestampLine}`);
}

export async function submoduleUpdate() {
    await utils.exec('git submodule init');
    await utils.exec('git submodule update');
}

async function checkEnv() {
    const tools = ['node', 'yarn', 'docker', 'docker compose', 'cargo'];
    for (const tool of tools) {
        await utils.exec(`which ${tool}`);
    }
    const { stdout: version } = await utils.exec('node --version');
    // Node v14.14 is required because
    // the `fs.rmSync` function was added in v14.14.0
    if ('v14.14' >= version) {
        throw new Error('Error, node.js version 14.14.0 or higher is required');
    }
}

export const initCommand = new Command('init')
    .option('--skip-submodules-checkout')
    .description('perform zksync network initialization for development')
    .action(async (cmd: Command) => {
        await init(cmd.skipSubmodulesCheckout);
    });
export const reinitCommand = new Command('reinit')
    .description('"reinitializes" network. Runs faster than `init`, but requires `init` to be executed prior')
    .action(reinit);
export const lightweightInitCommand = new Command('lightweight-init')
    .description('perform lightweight zksync network initialization for development')
    .action(lightweightInit);

import { Command } from 'commander';

import * as utils from './utils';

import { clean } from './clean';
import * as compiler from './compiler';
import * as contract from './contract';
import * as db from './database';
import * as docker from './docker';
import * as env from './env';
import * as run from './run';
import * as server from './server';
import { up } from './up';
import { announced } from './utils';

export const ADDRESS_ONE = '0x0000000000000000000000000000000000000001';

// Checks if all required tools are installed with the correct versions
const checkEnv = async (): Promise<void> => {
    const tools = ['node', 'yarn', 'docker', 'cargo'];
    for (const tool of tools) {
        await utils.exec(`which ${tool}`);
    }
    const { stdout: nodeVersion } = await utils.exec('node --version');
    if ('v18.18.0' >= nodeVersion) {
        throw new Error('Error, node.js version 18.18.0 or higher is required');
    }
    const { stdout: yarnVersion } = await utils.exec('yarn --version');
    if ('1.22.0' >= yarnVersion) {
        throw new Error('Error, yarn version 1.22.0 is required');
    }
};

// Initializes and updates the git submodule
const submoduleUpdate = async (): Promise<void> => {
    await utils.exec('git submodule init');
    await utils.exec('git submodule update');
};

// Sets up docker environment and compiles contracts
type InitSetupOptions = { skipEnvSetup: boolean; skipSubmodulesCheckout: boolean };
export const initSetup = async ({ skipSubmodulesCheckout, skipEnvSetup }: InitSetupOptions): Promise<void> => {
    if (!process.env.CI && !skipEnvSetup) {
        await announced('Pulling images', docker.pull());
        await announced('Checking environment', checkEnv());
        await announced('Checking git hooks', env.gitHooks());
        await announced('Setting up containers', up());
    }
    if (!skipSubmodulesCheckout) {
        await announced('Checkout submodules', submoduleUpdate());
    }

    await announced('Compiling JS packages', run.yarn());
    await announced('Building L1 L2 contracts', contract.build());
    await announced('Compile L2 system contracts', compiler.compileAll());
};

// Sets up the database, deploys the verifier (if set) and runs server genesis
type InitDatabaseOptions = { skipVerifierDeployment: boolean };
const initDatabase = async ({ skipVerifierDeployment }: InitDatabaseOptions): Promise<void> => {
    await announced('Drop postgres db', db.drop({ server: true, prover: true }));
    await announced('Setup postgres db', db.setup({ server: true, prover: true }));
    await announced('Clean rocksdb', clean(`db/${process.env.ZKSYNC_ENV!}`));
    await announced('Clean backups', clean(`backups/${process.env.ZKSYNC_ENV!}`));

    if (!skipVerifierDeployment) {
        await announced('Deploying L1 verifier', contract.deployVerifier());
    }

    await announced('Running server genesis setup', server.genesisFromSources());
};

// Deploys ERC20 and WETH tokens to localhost
const deployTestTokens = async () => {
    await announced('Deploying localhost ERC20 and Weth tokens', run.deployERC20AndWeth({ command: 'dev' }));
};

// Deploys and verifies L1 contracts and initializes governance
const initBridgehubStateTransition = async () => {
    await announced('Deploying L1 contracts', contract.deployL1());
    await announced('Verifying L1 contracts', contract.verifyL1Contracts());
    await announced('Initializing governance', contract.initializeGovernance());
    await announced('Reloading env', env.reload());
};

// Registers a hyperchain and deploys L2 contracts through L1
type InitHyperchainOptions = { includePaymaster: boolean; baseToken: { name: string; address: string } };
const initHyperchain = async ({ includePaymaster, baseToken }: InitHyperchainOptions): Promise<void> => {
    await announced('Registering Hyperchain', contract.registerHyperchain({ baseToken }));
    await announced('Deploying L2 contracts', contract.deployL2ThroughL1({ includePaymaster }));
};

// ########################### Command Actions ###########################
type InitDevCmdActionOptions = InitSetupOptions;
export const initDevCmdAction = async ({
    skipEnvSetup,
    skipSubmodulesCheckout
}: InitDevCmdActionOptions): Promise<void> => {
    await initSetup({ skipEnvSetup, skipSubmodulesCheckout });
    await initDatabase({ skipVerifierDeployment: false });
    await deployTestTokens();
    await initBridgehubStateTransition();
    await initHyperchain({ includePaymaster: true, baseToken: { name: 'ETH', address: ADDRESS_ONE } });
};

const lightweightInitCmdAction = async (): Promise<void> => {
    await announced('Clean rocksdb', clean('db'));
    await announced('Clean backups', clean('backups'));
    await announced('Deploying L1 verifier', contract.deployVerifier());
    await announced('Reloading env', env.reload());
    await announced('Running server genesis setup', server.genesisFromBinary());
    await announced('Deploying localhost ERC20 and Weth tokens', run.deployERC20AndWeth({ command: 'dev' }));
    await announced('Deploying L1 contracts', contract.redeployL1());
    await announced('Deploying L2 contracts', contract.deployL2ThroughL1({ includePaymaster: true }));
    await announced('Initializing governance', contract.initializeGovernance());
};

type InitSharedBridgeCmdActionOptions = InitSetupOptions;
const initSharedBridgeCmdAction = async (options: InitSharedBridgeCmdActionOptions): Promise<void> => {
    await initSetup(options);
    await initDatabase({ skipVerifierDeployment: false });
    await initBridgehubStateTransition();
};

type InitHyperCmdActionOptions = { skipSetupCompletely: boolean };
export const initHyperCmdAction = async ({ skipSetupCompletely }: InitHyperCmdActionOptions): Promise<void> => {
    if (!skipSetupCompletely) {
        await initSetup({ skipEnvSetup: false, skipSubmodulesCheckout: false });
    }
    await initDatabase({ skipVerifierDeployment: true });
    await initHyperchain({ includePaymaster: true, baseToken: { name: 'ETH', address: ADDRESS_ONE } });
};

// ########################### Command Definitions ###########################
export const initCommand = new Command('init')
    .option('--skip-submodules-checkout')
    .option('--skip-env-setup')
    .description('Deploys the shared bridge and registers a hyperchain locally, as quickly as possible.')
    .action(initDevCmdAction);

initCommand
    .command('lightweight')
    .description('A lightweight `init` that sets up local databases, generates genesis and deploys contracts.')
    .action(lightweightInitCmdAction);

initCommand
    .command('shared-bridge')
    .description('Deploys only the shared bridge and initializes governance. It does not deploy L2 contracts.')
    .option('--skip-submodules-checkout')
    .action(initSharedBridgeCmdAction);

initCommand
    .command('hyper')
    .description('Registers a hyperchain and deploys L2 contracts only. It requires an already deployed shared bridge.')
    .option('--skip-setup-completely', 'skip the setup completely, use this if server was started already')
    .action(initHyperCmdAction);

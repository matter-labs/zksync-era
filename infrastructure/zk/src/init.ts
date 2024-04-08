import { Command } from 'commander';

import * as utils from './utils';
import { announced } from './utils';

import { clean } from './clean';
import * as compiler from './compiler';
import * as contract from './contract';
import { DeploymentMode } from './contract';
import * as db from './database';
import * as docker from './docker';
import * as env from './env';
import * as config from './config';
import * as run from './run';
import * as server from './server';
import { createVolumes, up } from './up';

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
type InitSetupOptions = {
    skipEnvSetup: boolean;
    skipSubmodulesCheckout: boolean;
    runObservability: boolean;
    deploymentMode: DeploymentMode;
};
const initSetup = async ({
    skipSubmodulesCheckout,
    skipEnvSetup,
    runObservability,
    deploymentMode
}: InitSetupOptions): Promise<void> => {
    if (!process.env.CI && !skipEnvSetup) {
        await announced('Pulling images', docker.pull());
        await announced('Checking environment', checkEnv());
        await announced('Checking git hooks', env.gitHooks());
        await announced('Create volumes', createVolumes());
        await announced('Setting up containers', up(runObservability));
    }
    if (!skipSubmodulesCheckout) {
        await announced('Checkout submodules', submoduleUpdate());
    }
    if (deploymentMode == contract.DeploymentMode.Validium) {
        await announced('Checkout era-contracts for Validium mode', validiumSubmoduleCheckout());
    }

    await Promise.all([
        announced('Compiling JS packages', run.yarn()),
        announced('Building L1 L2 contracts', contract.build()),
        announced('Compile L2 system contracts', compiler.compileAll())
    ]);
};

// Sets up the database, deploys the verifier (if set) and runs server genesis
type InitDatabaseOptions = { skipVerifierDeployment: boolean };
const initDatabase = async ({ skipVerifierDeployment }: InitDatabaseOptions): Promise<void> => {
    await announced('Drop postgres db', db.drop({ core: true, prover: true }));
    await announced('Setup postgres db', db.setup({ core: true, prover: true }));
    await announced('Clean rocksdb', clean(`db/${process.env.ZKSYNC_ENV!}`));
    await announced('Clean backups', clean(`backups/${process.env.ZKSYNC_ENV!}`));

    if (!skipVerifierDeployment) {
        await announced('Deploying L1 verifier', contract.deployVerifier());
    }
};

// Deploys ERC20 and WETH tokens to localhost
type DeployTestTokensOptions = { envFile?: string };
const deployTestTokens = async (options?: DeployTestTokensOptions) => {
    await announced(
        'Deploying localhost ERC20 and Weth tokens',
        run.deployERC20AndWeth({ command: 'dev', envFile: options?.envFile })
    );
};

// Deploys and verifies L1 contracts and initializes governance
const initBridgehubStateTransition = async () => {
    await announced('Deploying L1 contracts', contract.deployL1(['']));
    await announced('Verifying L1 contracts', contract.verifyL1Contracts());
    await announced('Initializing governance', contract.initializeGovernance());
    await announced('Reloading env', env.reload());
};

// Registers a hyperchain and deploys L2 contracts through L1
type InitHyperchainOptions = { includePaymaster: boolean; baseTokenName?: string };
const initHyperchain = async ({ includePaymaster, baseTokenName }: InitHyperchainOptions): Promise<void> => {
    await announced('Registering Hyperchain', contract.registerHyperchain({ baseTokenName }));
    await announced('Running server genesis setup', server.genesisFromSources());
    await announced('Deploying L2 contracts', contract.deployL2ThroughL1({ includePaymaster }));
};

// ########################### Command Actions ###########################
type InitDevCmdActionOptions = InitSetupOptions & {
    skipTestTokenDeployment?: boolean;
    testTokenOptions?: DeployTestTokensOptions;
    baseTokenName?: string;
};
export const initDevCmdAction = async ({
    skipEnvSetup,
    skipSubmodulesCheckout,
    skipTestTokenDeployment,
    testTokenOptions,
    baseTokenName,
    runObservability,
    deploymentMode
}: InitDevCmdActionOptions): Promise<void> => {
    await initSetup({ skipEnvSetup, skipSubmodulesCheckout, runObservability, deploymentMode });
    await initDatabase({ skipVerifierDeployment: false });
    if (!skipTestTokenDeployment) {
        await deployTestTokens(testTokenOptions);
    }
    await initBridgehubStateTransition();
    await initDatabase({ skipVerifierDeployment: true });
    await initHyperchain({ includePaymaster: true, baseTokenName });
};

const lightweightInitCmdAction = async (): Promise<void> => {
    await announced('Clean rocksdb', clean('db'));
    await announced('Clean backups', clean('backups'));
    await announced('Deploying L1 verifier', contract.deployVerifier());
    await announced('Reloading env', env.reload());
    await announced('Running server genesis setup', server.genesisFromBinary());
    await announced('Deploying localhost ERC20 and Weth tokens', run.deployERC20AndWeth({ command: 'dev' }));
    // TODO set proper values
    await announced('Deploying L1 contracts', contract.redeployL1(false, DeploymentMode.Rollup));
    await announced('Deploying L2 contracts', contract.deployL2ThroughL1({ includePaymaster: true }));
    await announced('Initializing governance', contract.initializeGovernance());
};

export async function validiumSubmoduleCheckout() {
    await utils.exec(`cd contracts && git checkout origin/feat_validium_mode`);
}

type InitSharedBridgeCmdActionOptions = InitSetupOptions;
const initSharedBridgeCmdAction = async (options: InitSharedBridgeCmdActionOptions): Promise<void> => {
    await initSetup(options);
    await initDatabase({ skipVerifierDeployment: false });
    await initBridgehubStateTransition();
};

type InitHyperCmdActionOptions = {
    skipSetupCompletely: boolean;
    bumpChainId: boolean;
    baseTokenName?: string;
    runObservability: boolean;
    deploymentMode: DeploymentMode;
};
export const initHyperCmdAction = async ({
    skipSetupCompletely,
    bumpChainId,
    baseTokenName,
    runObservability,
    deploymentMode
}: InitHyperCmdActionOptions): Promise<void> => {
    if (bumpChainId) {
        await config.bumpChainId();
    }
    if (!skipSetupCompletely) {
        await initSetup({ skipEnvSetup: false, skipSubmodulesCheckout: false, runObservability, deploymentMode });
    }
    await initDatabase({ skipVerifierDeployment: true });
    await initHyperchain({ includePaymaster: true, baseTokenName });
};

// ########################### Command Definitions ###########################
export const initCommand = new Command('init')
    .option('--skip-submodules-checkout')
    .option('--skip-env-setup')
    .option('--base-token-name <base-token-name>', 'base token name')
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
    .option('--bump-chain-id', 'bump chain id to not conflict with previously deployed hyperchain')
    .option('--base-token-name <base-token-name>', 'base token name')
    .action(initHyperCmdAction);

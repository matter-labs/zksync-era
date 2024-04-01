import { Command } from 'commander';

import * as utils from './utils';

import { clean } from './clean';
import * as compiler from './compiler';
import * as contract from './contract';
import * as db from './database';
import * as docker from './docker';
import * as env from './env';
import * as config from './config';
import * as run from './run';
import * as server from './server';
import { createVolumes, up } from './up';
import { announced } from './utils';
import { type } from 'os';

// Checks if all required tools are installed with the correct versions
const checkEnv = async (runObservability: boolean): Promise<void> => {
    const tools = ['node', 'yarn', 'docker', 'cargo'];
    if (runObservability) {
        tools.push('yq');
    }
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

export async function validiumSubmoduleCheckout() {
    await utils.exec(`cd contracts && git checkout origin/feat_validium_mode`);
}

// clone dockprom and zksync-era dashboards
export async function setupObservability() {
    // clone dockprom, era-observability repos and export era dashboards to dockprom
    await utils.spawn(
        `rm -rf ./target/dockprom && git clone https://github.com/stefanprodan/dockprom.git ./target/dockprom \
            && rm -rf ./target/era-observability && git clone https://github.com/matter-labs/era-observability ./target/era-observability \
            && cp ./target/era-observability/dashboards/* ./target/dockprom/grafana/provisioning/dashboards
        `
    );
    // add scrape configuration to prometheus
    await utils.spawn(
        `yq eval '.scrape_configs += [{"job_name": "zksync", "scrape_interval": "5s", "honor_labels": true, "static_configs": [{"targets": ["host.docker.internal:3312"]}]}]' \
            -i ./target/dockprom/prometheus/prometheus.yml
        `
    );
}

// Sets up docker environment and compiles contracts
type InitSetupOptions = {
    skipEnvSetup: boolean;
    skipSubmodulesCheckout: boolean;
    deploymentMode: contract.DeploymentMode;
    runObservability: boolean;
};
const initSetup = async ({
    skipSubmodulesCheckout,
    skipEnvSetup,
    deploymentMode,
    runObservability
}: InitSetupOptions): Promise<void> => {
    await announced(
        `Initializing in ${deploymentMode == contract.DeploymentMode.Validium ? 'Validium mode' : 'Roll-up mode'}`
    );

    if (runObservability) {
        await announced('Pulling observability repos', setupObservability());
    }

    if (!process.env.CI && !skipEnvSetup) {
        await announced('Pulling images', docker.pull());
        await announced('Checking environment', checkEnv(runObservability));
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
type InitDatabaseOptions = {
    skipVerifierDeployment: boolean;
    deployerPrivateKeyArgs: any[];
    deploymentMode: contract.DeploymentMode;
};
const initDatabase = async ({
    skipVerifierDeployment,
    deployerPrivateKeyArgs,
    deploymentMode
}: InitDatabaseOptions): Promise<void> => {
    await announced('Drop postgres db', db.drop({ core: true, prover: true }));
    await announced('Setup postgres db', db.setup({ core: true, prover: true }));
    await announced('Clean rocksdb', clean(`db/${process.env.ZKSYNC_ENV!}`));
    await announced('Clean backups', clean(`backups/${process.env.ZKSYNC_ENV!}`));

    if (!skipVerifierDeployment) {
        await announced('Deploying L1 verifier', contract.deployVerifier(deployerPrivateKeyArgs, deploymentMode));
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
type InitBridgehubOptions = { deployerPrivateKeyArgs: any[]; deploymentMode: contract.DeploymentMode };
const initBridgehubStateTransition = async ({ deployerPrivateKeyArgs, deploymentMode }: InitBridgehubOptions) => {
    await announced('Running server genesis setup', server.genesisFromSources({ setChainId: false }));
    await announced('Deploying L1 contracts', contract.deployL1(deployerPrivateKeyArgs, deploymentMode));
    await announced('Verifying L1 contracts', contract.verifyL1Contracts());
    await announced('Initializing governance', contract.initializeGovernance());
    await announced('Reloading env', env.reload());
};

// Registers a hyperchain and deploys L2 contracts through L1
type InitHyperchainOptions = { includePaymaster: boolean; baseTokenName?: string };
const initHyperchain = async ({ includePaymaster, baseTokenName }: InitHyperchainOptions): Promise<void> => {
    await announced('Registering Hyperchain', contract.registerHyperchain({ baseTokenName }));
    await announced('Running server genesis setup', server.genesisFromSources({ setChainId: true }));
    await announced('Deploying L2 contracts', contract.deployL2ThroughL1({ includePaymaster }));
};

// ########################### Command Actions ###########################
type InitDevCmdActionOptions = InitSetupOptions & {
    validiumMode: boolean;
    skipTestTokenDeployment?: boolean;
    testTokenOptions?: DeployTestTokensOptions;
    baseTokenName?: string;
};
export const initDevCmdAction = async ({
    skipEnvSetup,
    skipSubmodulesCheckout,
    validiumMode,
    runObservability,
    skipTestTokenDeployment,
    testTokenOptions,
    baseTokenName
}: InitDevCmdActionOptions): Promise<void> => {
    const deploymentMode = validiumMode ? contract.DeploymentMode.Validium : contract.DeploymentMode.Rollup;
    await initSetup({ skipEnvSetup, skipSubmodulesCheckout, deploymentMode, runObservability });
    await initDatabase({ skipVerifierDeployment: false, deploymentMode, deployerPrivateKeyArgs: [] });
    if (!skipTestTokenDeployment) {
        await deployTestTokens(testTokenOptions);
    }
    await initBridgehubStateTransition({ deployerPrivateKeyArgs: [], deploymentMode });
    await initDatabase({ skipVerifierDeployment: true, deployerPrivateKeyArgs: [], deploymentMode });
    await initHyperchain({ includePaymaster: true, baseTokenName });
};

type LightWeightInitOptions = { deployerPrivateKeyArgs: any[]; deploymentMode: contract.DeploymentMode };
const lightweightInitCmdAction = async ({
    deployerPrivateKeyArgs,
    deploymentMode
}: LightWeightInitOptions): Promise<void> => {
    await announced('Clean rocksdb', clean('db'));
    await announced('Clean backups', clean('backups'));
    await announced('Deploying L1 verifier', contract.deployVerifier(deployerPrivateKeyArgs, deploymentMode));
    await announced('Reloading env', env.reload());
    await announced('Running server genesis setup', server.genesisFromBinary());
    await announced('Deploying localhost ERC20 and Weth tokens', run.deployERC20AndWeth({ command: 'dev' }));
    await announced('Deploying L1 contracts', contract.redeployL1(deployerPrivateKeyArgs, deploymentMode));
    await announced('Deploying L2 contracts', contract.deployL2ThroughL1({ includePaymaster: true }));
    await announced('Initializing governance', contract.initializeGovernance());
};

type InitSharedBridgeCmdActionOptions = InitSetupOptions;
const initSharedBridgeCmdAction = async (options: InitSharedBridgeCmdActionOptions): Promise<void> => {
    await initSetup(options);
    await initDatabase({
        skipVerifierDeployment: false,
        deployerPrivateKeyArgs: [],
        deploymentMode: contract.DeploymentMode.Rollup
    });
    await initBridgehubStateTransition({ deployerPrivateKeyArgs: [], deploymentMode: contract.DeploymentMode.Rollup });
};

type InitHyperCmdActionOptions = {
    skipSetupCompletely: boolean;
    bumpChainId: boolean;
    baseTokenName?: string;
};
export const initHyperCmdAction = async ({
    skipSetupCompletely,
    bumpChainId,
    baseTokenName
}: InitHyperCmdActionOptions): Promise<void> => {
    if (bumpChainId) {
        await config.bumpChainId();
    }
    if (!skipSetupCompletely) {
        await initSetup({
            skipEnvSetup: false,
            skipSubmodulesCheckout: false,
            deploymentMode: contract.DeploymentMode.Rollup,
            runObservability: false
        });
    }
    await initDatabase({
        skipVerifierDeployment: true,
        deployerPrivateKeyArgs: [],
        deploymentMode: contract.DeploymentMode.Rollup
    });
    await initHyperchain({ includePaymaster: true, baseTokenName });
};

// ########################### Command Definitions ###########################
export const initCommand = new Command('init')
    .option('--skip-submodules-checkout')
    .option('--skip-env-setup')
    .option('--run-observability')
    .option('--validium-mode')
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
    .option('--run-observability')
    .option('--validium-mode')
    .option('--skip-setup-completely', 'skip the setup completely, use this if server was started already')
    .option('--bump-chain-id', 'bump chain id to not conflict with previously deployed hyperchain')
    .option('--base-token-name <base-token-name>', 'base token name')
    .action(initHyperCmdAction);

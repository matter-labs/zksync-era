import chalk from 'chalk';
import { Command } from 'commander';
import { ethers } from 'ethers';

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

const entry = chalk.bold.yellow;
const announce = chalk.yellow;
const success = chalk.green;
const timestamp = chalk.grey;
export const ADDRESS_ONE = '0x0000000000000000000000000000000000000001';

export async function initSetup(initArgs: InitArgs = DEFAULT_ARGS) {
    const { skipSubmodulesCheckout, skipEnvSetup } = initArgs;

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
}

export async function initSetupDatabase(initArgs: InitArgs = DEFAULT_ARGS, skipVerifierDeployment: boolean = false) {
    const { deployerL2ContractInput } = initArgs;

    await announced('Drop postgres db', db.drop({server: true, prover: true}));
    await announced('Setup postgres db', db.setup({server: true, prover: true}));
    await announced('Clean rocksdb', clean(`db/${process.env.ZKSYNC_ENV!}`));
    await announced('Clean backups', clean(`backups/${process.env.ZKSYNC_ENV!}`));
    if (!skipVerifierDeployment) {
        await announced('Deploying L1 verifier', contract.deployVerifier(deployerL2ContractInput.args));
        await announced('Reloading env', env.reload());
    }

    await announced('Running server genesis setup', server.genesisFromSources());
}

export async function initBridgehubStateTransition(initArgs: InitArgs = DEFAULT_ARGS) {
    const { governorPrivateKeyArgs, deployerL2ContractInput, testTokens } = initArgs;

    if (testTokens.deploy) {
        await announced(
            'Deploying localhost ERC20 and Weth tokens',
            run.deployERC20AndWeth('dev', '', '', '', testTokens.args)
        );
    } else if (testTokens.deployWeth) {
        await announced('Deploying localhost Weth tokens', run.deployWeth(testTokens.args));
    }
    await announced('Deploying L1 contracts', contract.redeployL1(governorPrivateKeyArgs));
    await announced('Initializing governance', contract.initializeGovernance(governorPrivateKeyArgs));
    await announced('Reloading env', env.reload());
}

export async function initHyper(initArgs: InitArgs = DEFAULT_ARGS) {
    const { governorPrivateKeyArgs, deployerL2ContractInput, baseToken } = initArgs;

    let baseTokenArgs: any[] = [];
    if (baseToken.address !== ethers.constants.AddressZero) {
        baseTokenArgs = ['--base-token-address', baseToken.address];
    } else if (baseToken.name) {
        baseTokenArgs = ['--base-token-name', baseToken.name];
    }
    await announced(
        'Registering Hyperchain',
        contract.registerHyperchain([...governorPrivateKeyArgs, ...baseTokenArgs])
    );
    await announced('Reloading env', env.reload());
    if (deployerL2ContractInput.throughL1) {
        await announced(
            'Deploying L2 contracts',
            contract.deployL2ThroughL1(deployerL2ContractInput.args, deployerL2ContractInput.includePaymaster)
        );
    }
}

// we keep the old function which deploys the shared bridge and registers the hyperchain as quickly as possible
export async function init(initArgs: InitArgs = DEFAULT_ARGS) {
    await initSetup(initArgs);
    // we have to initiate the db here, as we need to create the genesis block to initialize the L1 contracts
    await initSetupDatabase(initArgs, false);
    await initBridgehubStateTransition(initArgs);
    // we do not reinitalize the db here, as we can use the db that was initialized for the genesis block in the StateTransitionManager
    await initHyper(initArgs);
}

// A smaller version of `init` that "resets" the localhost environment, for which `init` was already called before.
// It does less and runs much faster.
export async function reinit() {
    await announced('Setting up containers', up());
    await announced('Compiling JS packages', run.yarn());
    await announced('Compile l2 contracts', compiler.compileAll());
    await announced('Drop postgres db', db.drop({ server: true, prover: true }));
    await announced('Setup postgres db', db.setup({ server: true, prover: true }));
    await announced('Clean rocksdb', clean(`db/${process.env.ZKSYNC_ENV!}`));
    await announced('Clean backups', clean(`backups/${process.env.ZKSYNC_ENV!}`));
    await announced('Building contracts', contract.build());
    //note no ERC20 tokens are deployed here
    await announced('Deploying L1 verifier', contract.deployVerifier([]));
    await announced('Reloading env', env.reload());
    await announced('Running server genesis setup', server.genesisFromSources());
    await announced('Deploying L1 contracts', contract.redeployL1([]));
    await announced('Deploying L2 contracts', contract.deployL2ThroughL1([], true));
    await announced('Initializing governance', contract.initializeGovernance());
}

// A lightweight version of `init` that sets up local databases, generates genesis and deploys precompiled contracts
export async function lightweightInit() {
    await announced('Clean rocksdb', clean('db'));
    await announced('Clean backups', clean('backups'));
    await announced('Deploying L1 verifier', contract.deployVerifier([]));
    await announced('Reloading env', env.reload());
    await announced('Running server genesis setup', server.genesisFromBinary());
    await announced('Deploying localhost ERC20 and Weth tokens', run.deployERC20AndWeth('dev', '', '', '', []));
    await announced('Deploying L1 contracts', contract.redeployL1([]));
    await announced('Deploying L2 contracts', contract.deployL2ThroughL1([], true));
    await announced('Initializing governance', contract.initializeGovernance());
}

export async function deployL2Contracts(initArgs: InitArgs = DEFAULT_ARGS) {}

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

export async function checkEnv() {
    const tools = ['node', 'yarn', 'docker', 'cargo'];
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

export interface InitArgs {
    skipSubmodulesCheckout: boolean;
    skipEnvSetup: boolean;
    governorPrivateKeyArgs: any[];
    deployerPrivateKeyArgs: any[];
    deployerL2ContractInput: {
        args: any[];
        throughL1: boolean;
        includePaymaster: boolean;
    };
    testTokens: {
        deploy: boolean;
        deployWeth: boolean;
        args: any[];
    };
    baseToken: {
        address: string;
        name?: string;
    };
}

const DEFAULT_ARGS: InitArgs = {
    skipSubmodulesCheckout: false,
    skipEnvSetup: false,
    governorPrivateKeyArgs: [],
    deployerL2ContractInput: { args: [], throughL1: true, includePaymaster: true },
    testTokens: { deploy: true, deployWeth: true, args: [] },
    baseToken: { name: 'ETH', address: ADDRESS_ONE },
    deployerPrivateKeyArgs: [],
};

export const initCommand = new Command('init')
    .option('--skip-submodules-checkout')
    .option('--skip-env-setup')
    .option('--env-name <env-name>', 'chain name to use for initialization')
    .option('--base-token-name <base-token-name>', 'base token name')
    .option('--base-token-address <base-token-address>', 'base token address')
    .description('perform zksync network initialization for development')
    .action(async (cmd: Command) => {
        if (cmd.envName) {
            process.env.ZKSYNC_ENV = cmd.envName;
            env.reload();
        }

        const initArgs: InitArgs = {
            skipSubmodulesCheckout: cmd.skipSubmodulesCheckout,
            skipEnvSetup: cmd.skipEnvSetup,
            governorPrivateKeyArgs: process.env.GOVERNOR_PRIVATE_KEY
                ? ['--private-key', process.env.GOVERNOR_PRIVATE_KEY]
                : [],
            deployerL2ContractInput: {
                args: process.env.DEPLOYER_PRIVATE_KEY ? ['--private-key', process.env.DEPLOYER_PRIVATE_KEY] : [],
                throughL1: true,
                includePaymaster: true
            },
            testTokens: {
                deploy: true,
                deployWeth: true,
                args: process.env.DEPLOYER_PRIVATE_KEY ? ['--private-key', process.env.DEPLOYER_PRIVATE_KEY] : []
            },
            baseToken: {
                name: cmd.baseTokenName,
                address: cmd.baseTokenAddress ? cmd.baseTokenAddress : ethers.constants.AddressZero
            },
            deployerPrivateKeyArgs: []
        };
        await init(initArgs);
    });
export const reinitCommand = new Command('reinit')
    .description('"reinitializes" network. Runs faster than `init`, but requires `init` to be executed prior')
    .option('--env-name <env-name>', 'env name to use for initialization')
    .action(async (cmd: Command) => {
        if (cmd.envName) {
            process.env.ZKSYNC_ENV = cmd.envName;
            env.reload();
        }
        await reinit();
    });
export const lightweightInitCommand = new Command('lightweight-init')
    .description('perform lightweight zksync network initialization for development')
    .option('--env-name <env-name>', 'env name to use for initialization')
    .action(async (cmd: Command) => {
        if (cmd.envName) {
            process.env.ZKSYNC_ENV = cmd.envName;
            env.reload();
        }
        await lightweightInit();
    });
export const initHyperCommand = new Command('init-hyper')
    .description('initialize just the L2, currently with own bridge')
    .option('--skip-setup-completely', 'skip the setup completely, use this if server was started already')
    .option('--env-name <env-name>', 'env name to use for initialization')
    .option('--base-token-name <base-token-name>', 'base token name')
    .option('--base-token-address <base-token-address>', 'base token address')
    .action(async (cmd: Command) => {
        if (cmd.envName) {
            process.env.ZKSYNC_ENV = cmd.envName;
            env.reload();
        }

        const initArgs: InitArgs = {
            skipSubmodulesCheckout: cmd.skipSubmodulesCheckout,
            skipEnvSetup: true, // either we are launching locally, in which case we have called init-shared-bridge, or we are launching to a testnet
            governorPrivateKeyArgs: process.env.GOVERNOR_PRIVATE_KEY
                ? ['--private-key', process.env.GOVERNOR_PRIVATE_KEY]
                : [],
            deployerL2ContractInput: {
                args: process.env.DEPLOYER_PRIVATE_KEY ? ['--private-key', process.env.DEPLOYER_PRIVATE_KEY] : [],
                throughL1: true,
                includePaymaster: true
            },
            testTokens: {
                deploy: false,
                deployWeth: false,
                args: process.env.DEPLOYER_PRIVATE_KEY ? ['--private-key', process.env.DEPLOYER_PRIVATE_KEY] : []
            },
            baseToken: {
                name: cmd.baseTokenName,
                // we use zero here to show that it is unspecified. If it were ether it would be one.
                address: cmd.baseTokenAddress ? cmd.baseTokenAddress : ethers.constants.AddressZero
            },
            deployerPrivateKeyArgs: [],
        };
        if (!cmd.skipSetupCompletely) {
            await initSetup(initArgs);
        }
        await initSetupDatabase(initArgs, true); // we skip Verifier deployment, it is only deployed with sharedBridge
        await initHyper(initArgs);
    });

export const reinitHyperCommand = new Command('reinit-hyper')
    .description(
        'for development purposes, can be called again after "init" if the server was not started, and is faster than "init-hyper"'
    )
    .option('--env-name <env-name>', 'env name to use for initialization')
    .option('--base-token-name <base-token-name>', 'base token name')
    .option('--base-token-address <base-token-address>', 'base token address')
    .action(async (cmd: Command) => {
        if (cmd.envName) {
            process.env.ZKSYNC_ENV = cmd.envName;
            env.reload();
        }

        const initArgs: InitArgs = {
            skipSubmodulesCheckout: cmd.skipSubmodulesCheckout,
            skipEnvSetup: cmd.skipEnvSetup,
            governorPrivateKeyArgs: process.env.GOVERNOR_PRIVATE_KEY
                ? ['--private-key', process.env.GOVERNOR_PRIVATE_KEY]
                : [],
            deployerL2ContractInput: {
                args: process.env.DEPLOYER_PRIVATE_KEY ? ['--private-key', process.env.DEPLOYER_PRIVATE_KEY] : [],
                throughL1: true,
                includePaymaster: true
            },
            testTokens: {
                deploy: false,
                deployWeth: false,
                args: process.env.DEPLOYER_PRIVATE_KEY ? ['--private-key', process.env.DEPLOYER_PRIVATE_KEY] : []
            },
            baseToken: {
                name: cmd.baseTokenName,
                // we use zero here to show that it is unspecified. If it is ether it is one.
                address: cmd.baseTokenAddress ? cmd.baseTokenAddress : ethers.constants.AddressZero
            },
            deployerPrivateKeyArgs: [],
        };
        await initHyper(initArgs);
    });
export const initSharedBridgeCommand = new Command('init-shared-bridge')
    .description('initialize just the L2, currently with own bridge')
    .option('--env-name <env-name>', 'env name to use for initialization')
    // .options('--no-test-tokens', 'if flag is set test tokens will not be deployed')
    .option('--base-token-name <base-token-name>', 'base token name')
    .option('--base-token-address <base-token-address>', 'base token address')
    .action(async (cmd: Command) => {
        if (cmd.envName) {
            process.env.ZKSYNC_ENV = cmd.envName;
            env.reload();
        }
        // const deployTestTokens = cmd.noTestTokens ? false : true; // kl todo this does not work for some reason
        const deployTestTokens = false;
        const deployWeth = false;
        const initArgs: InitArgs = {
            skipSubmodulesCheckout: cmd.skipSubmodulesCheckout,
            skipEnvSetup: cmd.skipEnvSetup,
            governorPrivateKeyArgs: process.env.GOVERNOR_PRIVATE_KEY
                ? ['--private-key', process.env.GOVERNOR_PRIVATE_KEY]
                : [],
            deployerL2ContractInput: {
                args: process.env.DEPLOYER_PRIVATE_KEY ? ['--private-key', process.env.DEPLOYER_PRIVATE_KEY] : [],
                throughL1: true,
                includePaymaster: true
            },
            testTokens: {
                deploy: deployTestTokens,
                deployWeth: deployWeth,
                args: process.env.DEPLOYER_PRIVATE_KEY ? ['--private-key', process.env.DEPLOYER_PRIVATE_KEY] : []
            },
            baseToken: {
                name: cmd.baseTokenName,
                // we use zero here to show that it is unspecified. If it is ether would be one.
                address: cmd.baseTokenAddress ? cmd.baseTokenAddress : ethers.constants.AddressZero
            },
            deployerPrivateKeyArgs: [],
        };
        await initSetup(initArgs);
        // we have to initiate the db here, as we need to create the genesis block to initialize the L1 contracts
        await initSetupDatabase(initArgs, false);
        await initBridgehubStateTransition(initArgs);
    });

export const deployL2ContractsCommand = new Command('deploy-l2-contracts')
    .description('deploying l2 contracts once the hyperchain server is running')
    .option('--env-name <env-name>', 'env name to use for initialization')
    .action(async (cmd: Command) => {
        if (cmd.envName) {
            process.env.ZKSYNC_ENV = cmd.envName;
            env.reload();
        }
        const initArgs: InitArgs = {
            skipSubmodulesCheckout: true,
            skipEnvSetup: true,
            governorPrivateKeyArgs: process.env.GOVERNOR_PRIVATE_KEY
                ? ['--private-key', process.env.GOVERNOR_PRIVATE_KEY]
                : [],
            deployerL2ContractInput: {
                args: process.env.DEPLOYER_PRIVATE_KEY ? ['--private-key', process.env.DEPLOYER_PRIVATE_KEY] : [],
                throughL1: true,
                includePaymaster: true
            },
            testTokens: {
                deploy: false,
                deployWeth: false,
                args: process.env.DEPLOYER_PRIVATE_KEY ? ['--private-key', process.env.DEPLOYER_PRIVATE_KEY] : []
            },
            baseToken: {
                name: cmd.baseTokenName,
                // we use zero here to show that it is unspecified. If it is ether would be one.
                address: cmd.baseTokenAddress ? cmd.baseTokenAddress : ethers.constants.AddressZero
            },
            deployerPrivateKeyArgs: [],
        };

        await deployL2Contracts(initArgs);
    });

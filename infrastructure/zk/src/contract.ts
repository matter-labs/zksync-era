import { Command } from 'commander';
import * as utils from './utils';
import * as env from './env';
import fs from 'fs';

export async function build(zkSyncNetwork: boolean): Promise<void> {
    const additionalParams = zkSyncNetwork ? `CONTRACTS_BASE_NETWORK_ZKSYNC=true` : '';
    await utils.spawn(`${additionalParams} yarn l1-contracts build`);
    await utils.spawn('yarn l2-contracts build');
}

export async function prepareSyncLayer(): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];
    await utils.spawn(
        `CONTRACTS_BASE_NETWORK_ZKSYNC=true yarn l1-contracts prepare-sync-layer deploy-sync-layer-contracts ${args} | tee sync-layer-prep.log`
    );

    const paramsFromEnv = [
        `SYNC_LAYER_API_WEB3_JSON_RPC_HTTP_URL=${process.env.API_WEB3_JSON_RPC_HTTP_URL}`,
        `SYNC_LAYER_CHAIN_ID=${process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID}`
    ].join('\n');

    const deployLog =
        fs
            .readFileSync('sync-layer-prep.log')
            .toString()
            .replace(/CONTRACTS/g, 'SYNC_LAYER') +
        '\n' +
        paramsFromEnv;
    const syncLayerEnvVars = [
        'SYNC_LAYER_CREATE2_FACTORY_ADDR',

        'SYNC_LAYER_STATE_TRANSITION_PROXY_ADDR',
        'SYNC_LAYER_STATE_TRANSITION_IMPL_ADDR',

        'SYNC_LAYER_DIAMOND_INIT_ADDR',
        'SYNC_LAYER_DEFAULT_UPGRADE_ADDR',
        'SYNC_LAYER_GENESIS_UPGRADE_ADDR',
        'SYNC_LAYER_GOVERNANCE_ADDR',
        'SYNC_LAYER_ADMIN_FACET_ADDR',
        'SYNC_LAYER_EXECUTOR_FACET_ADDR',
        'SYNC_LAYER_GETTERS_FACET_ADDR',
        'SYNC_LAYER_MAILBOX_FACET_ADDR',

        'SYNC_LAYER_VERIFIER_ADDR',
        'SYNC_LAYER_VALIDATOR_TIMELOCK_ADDR',

        'SYNC_LAYER_TRANSPARENT_PROXY_ADMIN_ADDR',

        'SYNC_LAYER_L1_MULTICALL3_ADDR',
        'SYNC_LAYER_BLOB_VERSIONED_HASH_RETRIEVER_ADDR',

        'SYNC_LAYER_API_WEB3_JSON_RPC_HTTP_URL',
        'SYNC_LAYER_CHAIN_ID'
    ];

    const envFile = `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`;

    console.log('Writing to', envFile);

    const updatedContracts = updateContractsEnv(envFile, deployLog, syncLayerEnvVars);

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    // FIXME: either use it the same way as above or remove it
    fs.writeFileSync('deployed_sync_layer_contracts.log', updatedContracts);
}

async function registerSyncLayer() {
    await utils.spawn(`CONTRACTS_BASE_NETWORK_ZKSYNC=true yarn l1-contracts prepare-sync-layer register-sync-layer`);
}

async function migrateToSyncLayer() {
    await utils.spawn(`CONTRACTS_BASE_NETWORK_ZKSYNC=true yarn l1-contracts prepare-sync-layer migrate-to-sync-layer`);
}

async function prepareValidatorsOnSyncLayer() {
    await utils.spawn(`CONTRACTS_BASE_NETWORK_ZKSYNC=true yarn l1-contracts prepare-sync-layer prepare-validators`);
}

async function recoverFromFailedMigrationToSyncLayer(failedTxSLHash: string) {
    await utils.spawn(
        `CONTRACTS_BASE_NETWORK_ZKSYNC=true yarn l1-contracts prepare-sync-layer recover-from-failed-migration --failed-tx-l2-hash ${failedTxSLHash}`
    );
}

export async function verifyL1Contracts(): Promise<void> {
    // Spawning a new script is expensive, so if we know that publishing is disabled, it's better to not launch
    // it at all (even though `verify` checks the network as well).
    if (utils.isCurrentNetworkLocal()) {
        console.log('Skip contract verification on localhost');
        return;
    }
    await utils.spawn('yarn l1-contracts verify');
}

export function updateContractsEnv(initEnv: string, deployLog: String, envVars: Array<string>): string {
    let updatedContracts = '';
    for (const envVar of envVars) {
        const pattern = new RegExp(`${envVar}=.*`, 'g');
        const matches = deployLog.match(pattern);
        if (matches !== null) {
            const varContents = matches[0];
            env.modify(envVar, varContents, initEnv, false);
            updatedContracts += `${varContents}\n`;
        }
    }
    env.reload();
    return updatedContracts;
}

export async function initializeValidator(args: any[]): Promise<void> {
    await utils.confirmAction();
    await utils.spawn(`yarn l1-contracts initialize-validator ${args.join(' ')} | tee initializeValidator.log`);
}

export async function initializeGovernance(): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.GOVERNANCE_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];

    await utils.spawn(`yarn l1-contracts initialize-governance ${args.join(' ')} | tee initializeGovernance.log`);
}

export async function deployL2(args: any[] = [], includePaymaster?: boolean): Promise<void> {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    if (!isLocalSetup) {
        await utils.spawn(`yarn l2-contracts build`);
    }

    await utils.spawn(`yarn l2-contracts deploy-shared-bridge-on-l2 ${args.join(' ')} | tee deployL2.log`);

    if (includePaymaster) {
        await utils.spawn(`yarn l2-contracts deploy-testnet-paymaster ${args.join(' ')} | tee -a deployL2.log`);
    }

    await utils.spawn(`yarn l2-contracts deploy-force-deploy-upgrader ${args.join(' ')} | tee -a deployL2.log`);

    let l2DeployLog = fs.readFileSync('deployL2.log').toString();
    const l2DeploymentEnvVars = [
        'CONTRACTS_L2_SHARED_BRIDGE_ADDR',
        'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR',
        'CONTRACTS_L2_DEFAULT_UPGRADE_ADDR'
    ];
    updateContractsEnv(`etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`, l2DeployLog, l2DeploymentEnvVars);
}

// for testnet and development purposes it is ok to deploy contracts form L1.
export async function deployL2ThroughL1({
    includePaymaster,
    localLegacyBridgeTesting
}: {
    includePaymaster: boolean;
    localLegacyBridgeTesting?: boolean;
}): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    if (!isLocalSetup) {
        await utils.spawn(`yarn l2-contracts build`);
    }

    await utils.spawn(
        `yarn l2-contracts deploy-shared-bridge-on-l2-through-l1 ${args.join(' ')} ${
            localLegacyBridgeTesting ? '--local-legacy-bridge-testing' : ''
        } | tee deployL2.log`
    );

    if (includePaymaster) {
        await utils.spawn(
            `yarn l2-contracts deploy-testnet-paymaster-through-l1 ${args.join(' ')} | tee -a deployL2.log`
        );
    }

    await utils.spawn(
        `yarn l2-contracts deploy-force-deploy-upgrader-through-l1 ${args.join(' ')} | tee -a deployL2.log`
    );

    let l2DeployLog = fs.readFileSync('deployL2.log').toString();
    const l2DeploymentEnvVars = [
        'CONTRACTS_L2_SHARED_BRIDGE_ADDR',
        'CONTRACTS_L2_ERC20_BRIDGE_ADDR',
        'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR',
        'CONTRACTS_L2_DEFAULT_UPGRADE_ADDR'
    ];
    updateContractsEnv(`etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`, l2DeployLog, l2DeploymentEnvVars);
    // erc20 bridge is now deployed as shared bridge, but we still need the config var:
    updateContractsEnv(
        `etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`,
        `CONTRACTS_L2_ERC20_BRIDGE_ADDR=${process.env.CONTRACTS_L2_SHARED_BRIDGE_ADDR}`,
        l2DeploymentEnvVars
    );
}

async function _deployL1(onlyVerifier: boolean, deploymentMode: DeploymentMode): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [
        privateKey ? `--private-key ${privateKey}` : '',
        onlyVerifier ? '--only-verifier' : '',
        deploymentMode == DeploymentMode.Validium ? '--validium' : ''
    ];

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.

    await utils.spawn(`yarn l1-contracts deploy-no-build ${args.join(' ')} | tee deployL1.log`);
    const deployLog = fs.readFileSync('deployL1.log').toString();
    const l1EnvVars = [
        'CONTRACTS_CREATE2_FACTORY_ADDR',

        'CONTRACTS_BRIDGEHUB_PROXY_ADDR',
        'CONTRACTS_BRIDGEHUB_IMPL_ADDR',

        'CONTRACTS_STATE_TRANSITION_PROXY_ADDR',
        'CONTRACTS_STATE_TRANSITION_IMPL_ADDR',

        'CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR',
        'CONTRACTS_DIAMOND_INIT_ADDR',
        'CONTRACTS_DEFAULT_UPGRADE_ADDR',
        'CONTRACTS_GENESIS_UPGRADE_ADDR',
        'CONTRACTS_GOVERNANCE_ADDR',
        'CONTRACTS_ADMIN_FACET_ADDR',
        'CONTRACTS_EXECUTOR_FACET_ADDR',
        'CONTRACTS_GETTERS_FACET_ADDR',
        'CONTRACTS_MAILBOX_FACET_ADDR',

        'CONTRACTS_VERIFIER_ADDR',
        'CONTRACTS_VALIDATOR_TIMELOCK_ADDR',

        'CONTRACTS_GENESIS_TX_HASH',
        'CONTRACTS_TRANSPARENT_PROXY_ADMIN_ADDR',
        'CONTRACTS_L1_SHARED_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_SHARED_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_ERC20_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_ERC20_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_WETH_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_WETH_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_ALLOW_LIST_ADDR',
        'CONTRACTS_L1_MULTICALL3_ADDR',
        'CONTRACTS_BLOB_VERSIONED_HASH_RETRIEVER_ADDR',

        /// temporary:
        'CONTRACTS_HYPERCHAIN_UPGRADE_ADDR'
    ];

    console.log('Writing to', `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`);
    const updatedContracts = updateContractsEnv(
        `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`,
        deployLog,
        l1EnvVars
    );

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('deployed_contracts.log', updatedContracts);
}

export enum DeploymentMode {
    Rollup = 0,
    Validium = 1
}

export async function redeployL1(verifierOnly: boolean, deploymentMode: DeploymentMode) {
    await _deployL1(verifierOnly, deploymentMode);
    await verifyL1Contracts();
}

export async function wethBridgeFinish(args: any[] = []): Promise<void> {
    await utils.confirmAction();
    await utils.spawn(`yarn l1-contracts weth-finish-deployment-on-chain ${args.join(' ')} | tee -a deployL2.log`);
}

export async function erc20BridgeFinish(args: any[] = []): Promise<void> {
    await utils.confirmAction();
    await utils.spawn(`yarn l1-contracts erc20-finish-deployment-on-chain ${args.join(' ')} | tee -a deployL2.log`);
}

export async function registerHyperchain({ baseTokenName }: { baseTokenName?: string }): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.GOVERNOR_PRIVATE_KEY;
    const args = [
        privateKey ? `--private-key ${privateKey}` : '',
        baseTokenName ? `--base-token-name ${baseTokenName}` : ''
    ];

    await utils.spawn(`yarn l1-contracts register-hyperchain ${args.join(' ')} | tee registerHyperchain.log`);
    const deployLog = fs.readFileSync('registerHyperchain.log').toString();

    const l2EnvVars = ['CHAIN_ETH_ZKSYNC_NETWORK_ID', 'CONTRACTS_DIAMOND_PROXY_ADDR', 'CONTRACTS_BASE_TOKEN_ADDR'];
    console.log('Writing to', `etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`);

    const updatedContracts = updateContractsEnv(
        `etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`,
        deployLog,
        l2EnvVars
    );

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('register_hyperchain.log', updatedContracts);
}

export async function deployVerifier(): Promise<void> {
    // Deploy mode doesn't matter here
    await _deployL1(true, DeploymentMode.Rollup);
}

export async function deployL1(args: [string]): Promise<void> {
    let mode;
    if (args.includes('validium')) {
        mode = DeploymentMode.Validium;
    } else {
        mode = DeploymentMode.Rollup;
    }
    await _deployL1(false, mode);
}

async function setupLegacyBridgeEra(): Promise<void> {
    await utils.confirmAction();
    if (process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID != process.env.CONTRACTS_ERA_CHAIN_ID) {
        throw new Error('Era chain and l2 chain id do not match');
    }
    process.env.CONTRACTS_ERA_DIAMOND_PROXY_ADDR = process.env.CONTRACTS_DIAMOND_PROXY_ADDR;

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];

    await utils.spawn(`yarn l1-contracts setup-legacy-bridge-era ${args.join(' ')} | tee setupLegacyBridgeEra.log`);

    const deployLog = fs.readFileSync('setupLegacyBridgeEra.log').toString();
    const l1EnvVars = ['CONTRACTS_L1_SHARED_BRIDGE_IMPL_ADDR'];

    console.log('Writing to', `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`);
    const updatedContracts = updateContractsEnv(
        `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`,
        deployLog,
        l1EnvVars
    );

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('upgraded_shared_bridge.log', updatedContracts);
}

export const command = new Command('contract').description('contract management');

command
    .command('redeploy [deploy-opts...]')
    .allowUnknownOption(true)
    .description('redeploy contracts')
    .action(redeployL1);
command.command('deploy [deploy-opts...]').allowUnknownOption(true).description('deploy contracts').action(deployL1);
command
    .command('build')
    .description('build contracts')
    .option('--zkSync', 'compile for zksync network')
    .action((cmd) => build(cmd.zkSync === true));

command
    .command('prepare-sync-layer')
    .description('prepare the network to server as a synclayer')
    .action(prepareSyncLayer);

command
    .command('register-sync-layer-counterpart')
    .description('prepare the network to server as a synclayer')
    .action(registerSyncLayer);

// zk contract migrate-to-sync-layer --sync-layer-chain-id 270 --sync-layer-url http://127.0.0.1:3050 --sync-layer-stm 0x0040D8c968E3d5C95B9b0C3A4F098A3Ce82929C9
command
    .command('migrate-to-sync-layer')
    .description('prepare the network to server as a synclayer')
    .action(async (cmd) => {
        await migrateToSyncLayer();
    });

// zk contract recover-from-migration --sync-layer-chain-id 270 --sync-layer-url http://127.0.0.1:3050 --failed-tx-l2-hash 0xcd23ebda8c3805a3ff8fba846a34218cb987cae3402f4150544b74032c9213e2
command
    .command('recover-from-migration')
    .description('recover from failed migration to sync layer')
    .option('--failed-tx-l2-hash <failedTxL2Hash>', 'the hash of the failed tx on the SL')
    .action(async (cmd) => {
        console.log('input params : ', cmd.failedTxL2Hash);
        await recoverFromFailedMigrationToSyncLayer(cmd.failedTxL2Hash);
    });


command
    .command('prepare-sync-layer-validators')
    .description('register hyperchain')
    .action(async () => {
        await prepareValidatorsOnSyncLayer();
    });

command.command('verify').description('verify L1 contracts').action(verifyL1Contracts);
command
    .command('setup-legacy-bridge-era')
    .description('upgrade shared bridge with deployed era diamond proxy address')
    .action(setupLegacyBridgeEra);
command
    .command('initialize-validator [init-opts...]')
    .allowUnknownOption(true)
    .description('initialize validator')
    .action(initializeValidator);
command
    .command('deploy-l2 [deploy-opts...]')
    .allowUnknownOption(true)
    .description('deploy l2 contracts')
    .action(deployL2);
command
    .command('initialize-governance [gov-opts...]')
    .allowUnknownOption(true)
    .description('initialize governance')
    .action(initializeGovernance);
command
    .command('register-hyperchain')
    .description('register hyperchain')
    .action(() => {
        registerHyperchain({});
    });
command
    .command('deploy-l2-through-l1')
    .description('deploy l2 through l1')
    .action(() => {
        deployL2ThroughL1({ includePaymaster: true });
    });
command.command('deploy-verifier').description('deploy verifier to l1').action(deployVerifier);

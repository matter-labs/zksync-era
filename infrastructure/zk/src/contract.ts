import { Command } from 'commander';
import * as utils from 'utils';
import * as env from './env';
import fs from 'fs';
import { Wallet } from 'ethers';
import path from 'path';

export async function build(zkSyncNetwork: boolean): Promise<void> {
    const additionalParams = zkSyncNetwork ? `CONTRACTS_BASE_NETWORK_ZKSYNC=true` : '';
    await utils.spawn(`${additionalParams} yarn l1-contracts build`);
    await utils.spawn('yarn l2-contracts build');
    await utils.spawn('yarn da-contracts build');
}

const syncLayerEnvVars = [
    'GATEWAY_CREATE2_FACTORY_ADDR',

    'GATEWAY_STATE_TRANSITION_PROXY_ADDR',
    'GATEWAY_STATE_TRANSITION_IMPL_ADDR',

    'GATEWAY_DIAMOND_INIT_ADDR',
    'GATEWAY_DEFAULT_UPGRADE_ADDR',
    'GATEWAY_GENESIS_UPGRADE_ADDR',
    'GATEWAY_GOVERNANCE_ADDR',
    'GATEWAY_ADMIN_FACET_ADDR',
    'GATEWAY_EXECUTOR_FACET_ADDR',
    'GATEWAY_GETTERS_FACET_ADDR',
    'GATEWAY_MAILBOX_FACET_ADDR',

    'GATEWAY_VERIFIER_ADDR',
    'GATEWAY_VALIDATOR_TIMELOCK_ADDR',

    // 'GATEWAY_TRANSPARENT_PROXY_ADMIN_ADDR',

    'GATEWAY_L1_MULTICALL3_ADDR',
    'GATEWAY_BLOB_VERSIONED_HASH_RETRIEVER_ADDR',

    'GATEWAY_API_WEB3_JSON_RPC_HTTP_URL',
    'GATEWAY_CHAIN_ID',

    'GATEWAY_BRIDGEHUB_IMPL_ADDR',
    'GATEWAY_BRIDGEHUB_PROXY_ADDR',

    // 'GATEWAY_TRANSPARENT_PROXY_ADMIN_ADDR',

    // 'GATEWAY_L1_SHARED_BRIDGE_IMPL_ADDR',
    // 'GATEWAY_L1_SHARED_BRIDGE_PROXY_ADDR',
    // 'GATEWAY_L1_ERC20_BRIDGE_IMPL_ADDR',
    // 'GATEWAY_L1_ERC20_BRIDGE_PROXY_ADDR',
    'CONTRACTS_STM_ASSET_INFO',

    'GATEWAY_DIAMOND_PROXY_ADDR',
    'GATEWAY_L1_RELAYED_SL_DA_VALIDATOR'
];

const USER_FACING_ENV_VARS = ['CONTRACTS_USER_FACING_DIAMOND_PROXY_ADDR', 'CONTRACTS_USER_FACING_BRIDGEHUB_PROXY_ADDR'];

export async function prepareSyncLayer(): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];
    await utils.spawn(
        `CONTRACTS_BASE_NETWORK_ZKSYNC=true yarn l1-contracts sync-layer deploy-sync-layer-contracts ${args} | tee sync-layer-prep.log`
    );

    const paramsFromEnv = [
        `GATEWAY_API_WEB3_JSON_RPC_HTTP_URL=${process.env.API_WEB3_JSON_RPC_HTTP_URL}`,
        `GATEWAY_CHAIN_ID=${process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID}`
    ].join('\n');

    const deployLog =
        fs
            .readFileSync('sync-layer-prep.log')
            .toString()
            .replace(/CONTRACTS/g, 'GATEWAY') +
        '\n' +
        paramsFromEnv;

    const envFile = `etc/env/l1-inits/${process.env.ZKSYNC_ENV!}-sync-layer.env`;

    console.log('Writing to', envFile);

    const updatedContracts = updateContractsEnv(envFile, deployLog, syncLayerEnvVars);

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    // FIXME: either use it the same way as above or remove it
    fs.writeFileSync('deployed_sync_layer_contracts.log', updatedContracts);
}

async function registerSyncLayer() {
    await utils.spawn(`CONTRACTS_BASE_NETWORK_ZKSYNC=true yarn l1-contracts sync-layer register-sync-layer`);
}

async function migrateToSyncLayer() {
    await utils.confirmAction();

    await utils.spawn(
        `CONTRACTS_BASE_NETWORK_ZKSYNC=true yarn l1-contracts sync-layer migrate-to-sync-layer | tee sync-layer-migration.log`
    );

    // TODO: potentially switch `ETH_SENDER_SENDER_MAX_AGGREGATED_TX_GAS` for local testing
    const migrationLog = fs
        .readFileSync('sync-layer-migration.log')
        .toString()
        .replace(/CONTRACTS/g, 'GATEWAY');

    const envFile = `etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`;
    console.log('Writing to', envFile);

    // FIXME: consider creating new sync_layer_* variable.
    updateContractsEnv(envFile, migrationLog, ['GATEWAY_DIAMOND_PROXY_ADDR']);
    env.modify('CONTRACTS_DIAMOND_PROXY_ADDR', process.env.GATEWAY_DIAMOND_PROXY_ADDR!, envFile, true);
    env.modify('ETH_SENDER_SENDER_PUBDATA_SENDING_MODE', 'RelayedL2Calldata', envFile, true);
    env.modify('ETH_SENDER_GAS_ADJUSTER_SETTLEMENT_MODE', 'Gateway', envFile, true);
}

async function prepareValidatorsOnSyncLayer() {
    await utils.spawn(`CONTRACTS_BASE_NETWORK_ZKSYNC=true yarn l1-contracts sync-layer prepare-validators`);
}

async function recoverFromFailedMigrationToSyncLayer(failedTxSLHash: string) {
    await utils.spawn(
        `CONTRACTS_BASE_NETWORK_ZKSYNC=true yarn l1-contracts sync-layer recover-from-failed-migration --failed-tx-l2-hash ${failedTxSLHash}`
    );
}

/// FIXME: generally we should use a different approach for config maintaining within sync layer
/// the chain should retain both "sync_layer" and "contracts_" contracts and be able to switch between them
async function updateConfigOnSyncLayer() {
    const specialParams = ['GATEWAY_API_WEB3_JSON_RPC_HTTP_URL', 'GATEWAY_CHAIN_ID'];

    const envFile = `etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`;

    // for (const userVar of USER_FACING_ENV_VARS) {
    //     const originalVar = userVar.replace(/CONTRACTS_USER_FACING/g, 'CONTRACTS');
    //     env.modify(userVar, process.env[originalVar]!, envFile, false);
    // }

    console.log('a');

    for (const envVar of syncLayerEnvVars) {
        if (specialParams.includes(envVar)) {
            continue;
        }
        const contractsVar = envVar.replace(/GATEWAY/g, 'CONTRACTS');
        env.modify(contractsVar, process.env[envVar]!, envFile, false);
    }
    env.modify('BRIDGE_LAYER_WEB3_URL', process.env.ETH_CLIENT_WEB3_URL!, envFile, false);
    env.modify('ETH_CLIENT_WEB3_URL', process.env.GATEWAY_API_WEB3_JSON_RPC_HTTP_URL!, envFile, false);
    env.modify('L1_RPC_ADDRESS', process.env.ETH_CLIENT_WEB3_URL!, envFile, false);
    env.modify('ETH_CLIENT_CHAIN_ID', process.env.GATEWAY_CHAIN_ID!, envFile, false);

    env.modify('CHAIN_ETH_NETWORK', 'localhostL2', envFile, false);

    env.modify('CONTRACTS_BASE_NETWORK_ZKSYNC', 'true', envFile, false);
    env.modify('ETH_SENDER_SENDER_MAX_AGGREGATED_TX_GAS', '4294967295', envFile, false);

    env.modify('ETH_SENDER_SENDER_WAIT_CONFIRMATIONS', '0', envFile, false);
    env.modify('ETH_SENDER_SENDER_PUBDATA_SENDING_MODE', 'RelayedL2Calldata', envFile, false);
    env.modify('ETH_SENDER_GAS_ADJUSTER_SETTLEMENT_MODE', 'Gateway', envFile, false);

    // FIXME: while logically incorrect, it is temporarily needed to make the synclayer start
    fs.copyFileSync(
        `${process.env.ZKSYNC_HOME}/etc/tokens/localhost.json`,
        `${process.env.ZKSYNC_HOME}/etc/tokens/localhostL2.json`
    );

    env.reload();
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

    if (includePaymaster) {
        await utils.spawn(`yarn l2-contracts deploy-testnet-paymaster ${args.join(' ')} | tee -a deployL2.log`);
    }

    await utils.spawn(`yarn l2-contracts deploy-force-deploy-upgrader ${args.join(' ')} | tee -a deployL2.log`);

    let l2DeployLog = fs.readFileSync('deployL2.log').toString();
    const l2DeploymentEnvVars = [
        'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR',
        'CONTRACTS_L2_DEFAULT_UPGRADE_ADDR'
    ];
    updateContractsEnv(`etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`, l2DeployLog, l2DeploymentEnvVars);
}

// for testnet and development purposes it is ok to deploy contracts form L1.
export async function deployL2ThroughL1({
    includePaymaster = true,
    localLegacyBridgeTesting,
    deploymentMode
}: {
    includePaymaster: boolean;
    localLegacyBridgeTesting?: boolean;
    deploymentMode: DeploymentMode;
}): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    if (!isLocalSetup) {
        await utils.spawn(`yarn l2-contracts build`);
    }

    // The deployment of the L2 DA must be the first operation in the batch, since otherwise it wont be possible to commit it.
    const daArgs = [...args, deploymentMode == DeploymentMode.Validium ? '--validium-mode' : ''];
    await utils.spawn(
        `yarn l2-contracts deploy-l2-da-validator-on-l2-through-l1 ${daArgs.join(' ')} | tee deployL2.log`
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
        'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR',
        'CONTRACTS_L2_DEFAULT_UPGRADE_ADDR',
        'CONTRACTS_L1_DA_VALIDATOR_ADDR',
        'CONTRACTS_L2_DA_VALIDATOR_ADDR',
    ];
    updateContractsEnv(`etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`, l2DeployLog, l2DeploymentEnvVars);
    // erc20 bridge is now deployed as shared bridge, but we still need the config var:
}

async function _deployL1(onlyVerifier: boolean): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : '', onlyVerifier ? '--only-verifier' : ''];

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.

    await utils.spawn(`yarn l1-contracts deploy-no-build ${args.join(' ')} | tee deployL1.log`);
    const deployLog = fs.readFileSync('deployL1.log').toString();
    const l1EnvVars = [
        'CONTRACTS_CREATE2_FACTORY_ADDR',

        'CONTRACTS_BRIDGEHUB_PROXY_ADDR',
        'CONTRACTS_BRIDGEHUB_IMPL_ADDR',

        'CONTRACTS_MESSAGE_ROOT_PROXY_ADDR',
        'CONTRACTS_MESSAGE_ROOT_IMPL_ADDR',

        'CONTRACTS_STATE_TRANSITION_PROXY_ADDR',
        'CONTRACTS_STATE_TRANSITION_IMPL_ADDR',

        'CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR',
        'CONTRACTS_DIAMOND_INIT_ADDR',
        'CONTRACTS_DEFAULT_UPGRADE_ADDR',
        'CONTRACTS_GENESIS_UPGRADE_ADDR',
        'CONTRACTS_GOVERNANCE_ADDR',
        'CONTRACTS_CHAIN_ADMIN_ADDR',
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
        'CONTRACTS_L1_NATIVE_TOKEN_VAULT_IMPL_ADDR',
        'CONTRACTS_L1_NATIVE_TOKEN_VAULT_PROXY_ADDR',
        'CONTRACTS_L1_ERC20_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_ERC20_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_WETH_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_WETH_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_ALLOW_LIST_ADDR',
        'CONTRACTS_L1_MULTICALL3_ADDR',
        'CONTRACTS_BLOB_VERSIONED_HASH_RETRIEVER_ADDR',

        'CONTRACTS_L1_ROLLUP_DA_VALIDATOR',
        'CONTRACTS_L1_VALIDIUM_DA_VALIDATOR',
        'CONTRACTS_STM_DEPLOYMENT_TRACKER_IMPL_ADDR',
        'CONTRACTS_STM_DEPLOYMENT_TRACKER_PROXY_ADDR',
        'CONTRACTS_STM_ASSET_INFO',

        'CONTRACTS_L1_NULLIFIER_IMPL_ADDR',
        'CONTRACTS_L1_NULLIFIER_PROXY_ADDR',

        /// temporary:
        'CONTRACTS_HYPERCHAIN_UPGRADE_ADDR'
    ];

    const envFile = `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`;
    console.log('Writing to');
    const updatedContracts = updateContractsEnv(envFile, deployLog, l1EnvVars);

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('deployed_contracts.log', updatedContracts);
}

export enum DeploymentMode {
    Rollup = 0,
    Validium = 1
}

export async function redeployL1(verifierOnly: boolean) {
    await _deployL1(verifierOnly);
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

export async function registerHyperchain({
    baseTokenName,
    deploymentMode
}: {
    baseTokenName?: string;
    deploymentMode?: DeploymentMode;
}): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.GOVERNOR_PRIVATE_KEY;
    let tokenMultiplierSetterAddress = process.env.TOKEN_MULTIPLIER_SETTER_ADDRESS;

    if (baseTokenName && !tokenMultiplierSetterAddress) {
        const testConfigPath = path.join(process.env.ZKSYNC_HOME as string, `etc/test_config/constant`);
        const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
        // this is one of the rich accounts
        tokenMultiplierSetterAddress = Wallet.fromMnemonic(
            process.env.MNEMONIC ?? ethTestConfig.mnemonic,
            "m/44'/60'/0'/0/2"
        ).address;
        console.log(`Defaulting token multiplier setter address to ${tokenMultiplierSetterAddress}`);
    }

    const args = [
        privateKey ? `--private-key ${privateKey}` : '',
        baseTokenName ? `--base-token-name ${baseTokenName}` : '',
        deploymentMode == DeploymentMode.Validium ? '--validium-mode' : '',
        tokenMultiplierSetterAddress ? `--token-multiplier-setter-address ${tokenMultiplierSetterAddress}` : '',
        '--use-governance'
    ];
    await utils.spawn(`yarn l1-contracts register-hyperchain ${args.join(' ')} | tee registerHyperchain.log`);
    const deployLog = fs.readFileSync('registerHyperchain.log').toString();

    const l2EnvVars = ['CHAIN_ETH_ZKSYNC_NETWORK_ID', 'CONTRACTS_DIAMOND_PROXY_ADDR', 'CONTRACTS_BASE_TOKEN_ADDR'];
    const l2EnvFile = `etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`;
    console.log('Writing to', l2EnvFile);

    const updatedContracts = updateContractsEnv(l2EnvFile, deployLog, l2EnvVars);

    for (const userVar of USER_FACING_ENV_VARS) {
        const originalVar = userVar.replace(/CONTRACTS_USER_FACING/g, 'CONTRACTS');
        env.modify(userVar, process.env[originalVar]!, l2EnvFile, false);
    }

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('register_hyperchain.log', updatedContracts);
}

export async function deployVerifier(): Promise<void> {
    await _deployL1(true);
}

export async function deployL1(): Promise<void> {
    await _deployL1(false);
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
    .action(async () => {
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

command
    .command('update-config-for-sync-layer')
    .description('updates config to include the new contracts for sync layer')
    .action(async () => {
        await updateConfigOnSyncLayer();
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
    .option('--base-token-name <base-token-name>', 'base token name')
    .option('--deployment-mode <deployment-mode>', 'deploy contracts in Validium mode')
    .option(
        '--token-multiplier-setter-address <token-multiplier-setter-address>',
        'address of the token multiplier setter'
    )
    .action(registerHyperchain);
command
    .command('deploy-l2-through-l1')
    .description('deploy l2 through l1')
    .option(
        '--local-legacy-bridge-testing',
        'used to test LegacyBridge compatibility. The chain will have the same id as the era chain id, while eraChainId in L2SharedBridge will be 0'
    )
    .action(deployL2ThroughL1);
command.command('deploy-verifier').description('deploy verifier to l1').action(deployVerifier);

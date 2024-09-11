import { Command } from 'commander';
import * as utils from 'utils';
import * as config from '../config';
import deepExtend from 'deep-extend';

export async function all() {
    await server();
    await api();
    // have to kill server before running data-restore
    await utils.spawn('pkill zksync_server');
}

export async function api(bail: boolean = false) {
    const flag = bail ? ' --bail' : '';
    await utils.spawn('yarn ts-integration api-test' + flag);
}

export async function contractVerification(bail: boolean = false) {
    const flag = bail ? ' --bail' : '';
    await utils.spawn('yarn ts-integration contract-verification-test' + flag);
}

export async function server(options: string[] = []) {
    if (process.env.ZKSYNC_ENV?.startsWith('ext-node')) {
        process.env.ZKSYNC_WEB3_API_URL = `http://127.0.0.1:${process.env.EN_HTTP_PORT}`;
        process.env.ZKSYNC_WEB3_WS_API_URL = `ws://127.0.0.1:${process.env.EN_WS_PORT}`;
        process.env.ETH_CLIENT_WEB3_URL = process.env.EN_ETH_CLIENT_URL;

        // We need base configs for integration tests
        const baseConfigs = config.loadConfig('dev');
        const configs = config.collectVariables(deepExtend(baseConfigs, config.loadConfig(process.env.ZKSYNC_ENV)));

        process.env.CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT = configs.get('CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT');
        process.env.CHAIN_STATE_KEEPER_VALIDATION_COMPUTATIONAL_GAS_LIMIT = configs.get(
            'CHAIN_STATE_KEEPER_VALIDATION_COMPUTATIONAL_GAS_LIMIT'
        );
        process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID = configs.get('CHAIN_ETH_ZKSYNC_NETWORK_ID');
    }
    await utils.spawn('yarn ts-integration test ' + options.map((opt) => `'${opt}'`).join(' '));
}

export async function fees() {
    await utils.spawn('yarn ts-integration fee-test');
}

export async function revert(bail: boolean = false) {
    const flag = bail ? ' --bail' : '';
    await utils.spawn('yarn revert-test revert-and-restart-test' + flag);
}

export async function revert_en(bail: boolean = false) {
    const flag = bail ? ' --bail' : '';
    await utils.spawn('yarn revert-test revert-and-restart-test-en' + flag);
}

export async function upgrade(bail: boolean = false) {
    const flag = bail ? ' --bail' : '';
    await utils.spawn('yarn upgrade-test upgrade-test' + flag);
}

export const command = new Command('integration').description('zksync integration tests').alias('i');

command
    .command('all')
    .description('run all integration tests (no testkit)')
    .action(async () => {
        await all();
    });

command
    .command('server [options...]')
    .allowUnknownOption(true)
    .description('run server integration tests')
    .action(async (options: string[]) => {
        await server(options);
    });

command
    .command('fees')
    .description('run server integration tests')
    .action(async () => {
        await fees();
    });

command
    .command('revert')
    .description('run revert test')
    .option('--bail')
    .action(async (cmd: Command) => {
        await revert(cmd.bail);
    });
command
    .command('revert-en')
    .description('run EN revert test')
    .option('--bail')
    .action(async (cmd: Command) => {
        await revert_en(cmd.bail);
    });
command
    .command('upgrade')
    .description('run upgrade test')
    .option('--bail')
    .action(async (cmd: Command) => {
        await upgrade(cmd.bail);
    });
command
    .command('api')
    .description('run api integration tests')
    .option('--bail')
    .action(async (cmd: Command) => {
        await api(cmd.bail);
    });

command
    .command('contract-verification')
    .description('run contract verification tests')
    .option('--bail')
    .action(async (cmd: Command) => {
        await contractVerification(cmd.bail);
    });

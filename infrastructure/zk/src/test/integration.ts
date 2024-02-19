import { Command } from 'commander';
import * as utils from '../utils';
import * as contract from '../contract';
import * as run from '../run/run';
import * as compiler from '../compiler';
import * as config from '../config';

export async function all() {
    await server();
    await api();
    await rustSDK();
    // have to kill server before running data-restore
    await utils.spawn('pkill zksync_server');
    await run.dataRestore.checkExisting();
}

export async function api(bail: boolean = false) {
    const flag = bail ? ' --bail' : '';
    await utils.spawn('yarn ts-integration api-test' + flag);
}

export async function contractVerification(bail: boolean = false) {
    const flag = bail ? ' --bail' : '';
    await utils.spawn('yarn ts-integration contract-verification-test' + flag);
}

export async function snapshotsCreator(bail: boolean = false) {
    const flag = bail ? ' --bail' : '';
    await utils.spawn('yarn ts-integration snapshots-creator-test' + flag);
}

export async function server(options: string[] = []) {
    if (process.env.ZKSYNC_ENV?.startsWith('ext-node')) {
        process.env.ZKSYNC_WEB3_API_URL = `http://127.0.0.1:${process.env.EN_HTTP_PORT}`;
        process.env.ZKSYNC_WEB3_WS_API_URL = `ws://127.0.0.1:${process.env.EN_WS_PORT}`;
        process.env.ETH_CLIENT_WEB3_URL = process.env.EN_ETH_CLIENT_URL;

        const configs = config.collectVariables(config.loadConfig(process.env.ZKSYNC_ENV, true));

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

export async function upgrade(bail: boolean = false) {
    const flag = bail ? ' --bail' : '';
    await utils.spawn('yarn upgrade-test upgrade-test' + flag);
}

export async function withdrawalHelpers() {
    await utils.spawn('yarn ts-tests withdrawal-helpers-test');
}

export async function testkit(args: string[], timeout: number) {
    let containerID = '';
    const prevUrls = process.env.ETH_CLIENT_WEB3_URL?.split(',')[0];
    if (process.env.ZKSYNC_ENV == 'dev' && process.env.CI != '1') {
        const { stdout } = await utils.exec('docker run --rm -d -p 7545:8545 matterlabs/geth:latest fast');
        containerID = stdout;
        process.env.ETH_CLIENT_WEB3_URL = 'http://localhost:7545';
    }
    process.on('SIGINT', () => {
        console.log('interrupt received');
        // we have to emit this manually, as SIGINT is considered explicit termination
        process.emit('beforeExit', 130);
    });

    // set a timeout in case tests hang
    const timer = setTimeout(() => {
        console.log('Timeout reached!');
        process.emit('beforeExit', 1);
    }, timeout * 1000);
    timer.unref();

    // since we HAVE to make an async call upon exit,
    // the only solution is to use beforeExit hook
    // but be careful! this is not called upon explicit termination
    // e.g. on SIGINT or process.exit()
    process.on('beforeExit', async (code) => {
        if (process.env.ZKSYNC_ENV == 'dev' && process.env.CI != '1') {
            try {
                // probably should be replaced with child_process.execSync in future
                // to change the hook to program.on('exit', ...)
                await utils.exec(`docker kill ${containerID}`);
            } catch {
                console.error('Problem killing', containerID);
            }
            process.env.ETH_CLIENT_WEB3_URL = prevUrls;
            // this has to be here - or else we will call this hook again
            process.exit(code);
        }
    });

    process.env.CHAIN_ETH_NETWORK = 'test';
    await compiler.compileAll();
    await contract.build();

    await utils.spawn(`cargo run --release --bin zksync_testkit -- ${args.join(' ')}`);
}

export async function rustSDK() {
    await utils.spawn('cargo test -p zksync --release -- --ignored --test-threads=1');
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
    .command('upgrade')
    .description('run upgrade test')
    .option('--bail')
    .action(async (cmd: Command) => {
        await upgrade(cmd.bail);
    });

command
    .command('rust-sdk')
    .description('run rust SDK integration tests')
    .option('--with-server')
    .action(async () => {
        await rustSDK();
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

command
    .command('snapshots-creator')
    .description('run snapshots creator tests')
    .option('--bail')
    .action(async (cmd: Command) => {
        await snapshotsCreator(cmd.bail);
    });

command
    .command('testkit [options...]')
    .allowUnknownOption(true)
    .description('run testkit tests')
    .option('--offline')
    .action(async (options: string[], offline: boolean = false) => {
        if (offline) {
            process.env.SQLX_OFFLINE = 'true';
        }
        if (options.length == 0) {
            options.push('all');
        }

        await testkit(options, 6000);

        if (offline) {
            delete process.env.SQLX_OFFLINE;
        }
    });

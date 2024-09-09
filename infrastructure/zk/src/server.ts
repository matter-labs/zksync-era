import { Command } from 'commander';
import * as utils from 'utils';
import { clean } from './clean';
import fs from 'fs';
import * as path from 'path';
import * as db from './database';
import * as env from './env';
import dotenv from 'dotenv';
// import { time } from 'console';

export async function server(
    rebuildTree: boolean,
    uring: boolean,
    components?: string,
    timeToLive?: string,
    txAggregationPaused?: boolean
) {
    if (txAggregationPaused) {
        process.env.ETH_SENDER_SENDER_TX_AGGREGATION_PAUSED = 'true';
    }
    let options = '';
    if (uring) {
        options += '--features=rocksdb/io-uring';
    }
    if (rebuildTree || components) {
        options += ' --';
    }
    if (components) {
        options += ` --components=${components}`;
    }
    if (!timeToLive) {
        await utils.spawn(`cargo run --bin zksync_server --release ${options}`);
    } else {
        console.log('Starting server');
        const child = utils.background({
            command: `cargo run --bin zksync_server --release ${options}`,
            stdio: [null, 'inherit', 'inherit']
        });

        const promise = new Promise((resolve, reject) => {
            child.on('error', reject);
            child.on('close', (code, signal) => {
                signal == 'SIGKILL'
                    ? resolve(signal)
                    : reject(`Child process exited with code ${code} and signal ${signal}`);
            });
        });

        await utils.sleep(+timeToLive);

        console.log(`${+timeToLive} seconds passed, killing the server.`);

        // Kill the server after the time to live.
        process.kill(-child.pid!, 'SIGKILL');

        console.log('Waiting for the server to shut down.');

        // Now waiting for the graceful shutdown of the server.
        await promise;

        console.log('Server successfully shut down.');
    }
}

export async function externalNode(env: string, reinit: boolean = false, args: string[]) {
    if (process.env.ZKSYNC_ENV != 'ext-node') {
        console.warn(`WARNING: using ${process.env.ZKSYNC_ENV} environment for external node`);
        console.warn('If this is a mistake, set $ZKSYNC_ENV to "ext-node" or other environment');
    }

    // Set proper environment variables for external node.
    process.env.EN_BOOTLOADER_HASH = process.env.CHAIN_STATE_KEEPER_BOOTLOADER_HASH;
    process.env.EN_DEFAULT_AA_HASH = process.env.CHAIN_STATE_KEEPER_DEFAULT_AA_HASH;

    // On --reinit we want to reset RocksDB and Postgres before we start.
    if (reinit) {
        await utils.confirmAction();
        await db.reset({ core: true, prover: false });
        clean(path.dirname(process.env.EN_MERKLE_TREE_PATH!));
    }

    // FIXME: make it work with and without sync layer.
    // set the correct diamond proxy addresses
    const targetEnv = dotenv.parse(fs.readFileSync(`etc/env/target/${env}.env`));
    process.env.EN_SL_CLIENT_MAP = JSON.stringify([
        {
            chain_id: +targetEnv.CONTRACTS_ERA_CHAIN_ID!,
            rpc_url: targetEnv.BRIDGE_LAYER_WEB3_URL,
            diamond_proxy_address: targetEnv.CONTRACTS_USER_FACING_DIAMOND_PROXY_ADDR,
        },
        {
            chain_id: +targetEnv.GATEWAY_CHAIN_ID!,
            rpc_url: targetEnv.ETH_CLIENT_WEB3_URL,
            // FIXME: calculate with create2
            diamond_proxy_address: targetEnv.GATEWAY_DIAMOND_PROXY_ADDR,
        }
    ]);
    process.env.EN_SL_CHAIN_ID = targetEnv.GATEWAY_CHAIN_ID;
    process.env.EN_L2_CHAIN_ID = targetEnv.CHAIN_ETH_ZKSYNC_NETWORK_ID;
    process.env.EN_L1_CHAIN_ID = targetEnv.CONTRACTS_ERA_CHAIN_ID;
    process.env.EN_MAIN_NODE_URL = targetEnv.API_WEB3_JSON_RPC_HTTP_URL;
    process.env.EN_ETH_CLIENT_URL = targetEnv.ETH_CLIENT_WEB3_URL;

    console.log(`MAIN_NODE_URL for env ${env}: ${process.env.EN_MAIN_NODE_URL}`);
    console.log(`ETH_CLIENT_URL for env ${env}: ${process.env.EN_ETH_CLIENT_URL}`);
    console.log(`CLIENT_MAP for env ${env}: ${process.env.EN_SL_CLIENT_MAP}`);

    await utils.spawn(`cargo run --release --bin zksync_external_node -- ${args.join(' ')}`);
}

async function create_genesis(cmd: string) {
    await utils.confirmAction();
    await utils.spawn(`${cmd} | tee genesis.log`);

    const date = new Date();
    const [year, month, day, hour, minute, second] = [
        date.getFullYear(),
        date.getMonth(),
        date.getDate(),
        date.getHours(),
        date.getMinutes(),
        date.getSeconds()
    ];
    const label = `${process.env.ZKSYNC_ENV}-Genesis_gen-${year}-${month}-${day}-${hour}${minute}${second}`;
    fs.mkdirSync(`logs/${label}`, { recursive: true });
    fs.copyFileSync('genesis.log', `logs/${label}/genesis.log`);
}

export async function genesisFromSources() {
    // Note that that all the chains have the same chainId at genesis. It will be changed
    // via an upgrade transaction during the registration of the chain.
    await create_genesis('cargo run --bin zksync_server --release -- --genesis');
}

// FIXME: remove this option once it is removed from the server
async function clearL1TxsHistory() {
    // Note that that all the chains have the same chainId at genesis. It will be changed
    // via an upgrade transaction during the registration of the chain.
    await create_genesis('cargo run --bin zksync_server --release -- --clear-l1-txs-history');
}

export async function genesisFromBinary() {
    await create_genesis('zksync_server --genesis');
}

export const serverCommand = new Command('server')
    .description('start zksync server')
    .option('--genesis', 'generate genesis data via server')
    // FIXME: remove this option once it is removed from the server
    .option('--clear-l1-txs-history', 'clear l1 txs history')
    .option('--uring', 'enables uring support for RocksDB')
    .option('--components <components>', 'comma-separated list of components to run')
    .option('--chain-name <chain-name>', 'environment name')
    .option('--time-to-live <time-to-live>', 'time to live for the server')
    .option('--tx-aggregation-paused', 'pause tx aggregation')
    .action(async (cmd: Command) => {
        cmd.chainName ? env.reload(cmd.chainName) : env.load();
        if (cmd.genesis) {
            await genesisFromSources();
        } else if (cmd.clearL1TxsHistory) {
            await clearL1TxsHistory();
        } else {
            await server(cmd.rebuildTree, cmd.uring, cmd.components, cmd.timeToLive, cmd.txAggregationPaused);
        }
    });

export const enCommand = new Command('external-node')
    .description('start zksync external node')
    .option('--reinit', 'reset postgres and rocksdb before starting')
    .option('--env <env>', 'environment name of the main node', 'dev')
    .action(async (cmd: Command) => {
        await externalNode(cmd.env, cmd.reinit, cmd.args);
    });

// const fn = async () => {
//     const transactions: string[] = [];

//     const validateTx = (tx: string) => {};
//     const executeTx = (tx: string) => {};

//     // 1. Initialize batch params.

//     // 2. Validate and execute transactions:
//     for (const transaction of transactions) {
//         validateTx(transaction);
//         executeTx(transaction);
//     }

//     // 3. Distribute funds to the operator
//     // and compress the final state diffs.
// };

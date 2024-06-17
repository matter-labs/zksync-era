import { Command } from 'commander';
import * as utils from 'utils';
import { clean } from './clean';
import fs from 'fs';
import * as path from 'path';
import * as db from './database';
import * as env from './env';
// import { time } from 'console';

export async function server(
    rebuildTree: boolean,
    uring: boolean,
    components?: string,
    useNodeFramework?: boolean,
    timeToLive?: string
) {
    let options = '';
    if (uring) {
        options += '--features=rocksdb/io-uring';
    }
    if (rebuildTree || components || useNodeFramework) {
        options += ' --';
    }
    if (rebuildTree) {
        clean('db');
        options += ' --rebuild-tree';
    }
    if (components) {
        options += ` --components=${components}`;
    }
    if (useNodeFramework) {
        options += ' --use-node-framework';
    }
    if (!timeToLive) {
        await utils.spawn(`cargo run --bin zksync_server --release ${options}`);
    } else {
        console.log('Starting server');
        const child = utils.background(`cargo run --bin zksync_server --release ${options}`);

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

export async function externalNode(reinit: boolean = false, args: string[]) {
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
    .option('--rebuild-tree', 'rebuilds merkle tree from database logs', 'rebuild_tree')
    // FIXME: remove this option once it is removed from the server
    .option('--clear-l1-txs-history', 'clear l1 txs history')
    .option('--uring', 'enables uring support for RocksDB')
    .option('--components <components>', 'comma-separated list of components to run')
    .option('--chain-name <chain-name>', 'environment name')
    .option('--use-node-framework', 'use node framework for server')
    .option('--time-to-live <time-to-live>', 'time to live for the server')
    .action(async (cmd: Command) => {
        cmd.chainName ? env.reload(cmd.chainName) : env.load();
        if (cmd.genesis) {
            await genesisFromSources();
        } else if (cmd.clearL1TxsHistory) {
            await clearL1TxsHistory();
        } else {
            await server(cmd.rebuildTree, cmd.uring, cmd.components, cmd.useNodeFramework, cmd.timeToLive);
        }
    });

export const enCommand = new Command('external-node')
    .description('start zksync external node')
    .option('--reinit', 'reset postgres and rocksdb before starting')
    .action(async (cmd: Command) => {
        await externalNode(cmd.reinit, cmd.args);
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

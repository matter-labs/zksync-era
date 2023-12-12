import { Command } from 'commander';
import * as utils from './utils';
import * as env from './env';
import { clean } from './clean';
import fs from 'fs';
import { unloadInit } from './env';
import * as path from 'path';
import * as db from './database';
import { ChildProcess } from 'child_process';

export async function server(rebuildTree: boolean, uring: boolean, components?: string) {
    let options = '';
    if (uring) {
        options += '--features=rocksdb/io-uring';
    }
    if (rebuildTree || components) {
        options += ' --';
    }
    if (rebuildTree) {
        clean('db');
        options += ' --rebuild-tree';
    }
    if (components) {
        options += ` --components=${components}`;
    }
    await utils.spawn(`cargo run --bin zksync_server --release ${options}`);
}

export async function externalNode(reinit: boolean = false) {
    if (process.env.ZKSYNC_ENV != 'ext-node') {
        console.warn(`WARNING: using ${process.env.ZKSYNC_ENV} environment for external node`);
        console.warn('If this is a mistake, set $ZKSYNC_ENV to "ext-node" or other environment');
    }

    // Set proper environment variables for external node.
    process.env.EN_BOOTLOADER_HASH = process.env.CHAIN_STATE_KEEPER_BOOTLOADER_HASH;
    process.env.EN_DEFAULT_AA_HASH = process.env.CHAIN_STATE_KEEPER_DEFAULT_AA_HASH;

    // We don't need to have server-native variables in the config.
    unloadInit();

    // On --reinit we want to reset RocksDB and Postgres before we start.
    if (reinit) {
        await utils.confirmAction();
        await db.reset();
        clean(path.dirname(process.env.EN_MERKLE_TREE_PATH!));
    }

    await utils.spawn('cargo run --release --bin zksync_external_node');
}

export class IsolatedExternalNode {
    public readonly process: ChildProcess;

    public constructor(process: ChildProcess) {
        this.process = process;
    }
}

export async function isolatedExternalNode() {
    const randomSuffix = Math.floor(Math.random() * 1000000)
        .toString()
        .padStart(6, '0');
    const instanceName = `zksync_ext_node_${randomSuffix}`;
    fs.writeFileSync(path.join(process.env.ZKSYNC_HOME as string, 'instances_to_clean'), instanceName + '\n', {
        flag: 'a+'
    });

    const binaryPath = '/usr/bin/zksync_external_node';
    let dockerEnv = ` --env "INTEGRATION_TEST_NODE_BINARY_PATH=${binaryPath}" `;
    const extNodeEnvName = process.env.IN_DOCKER ? 'ext-node-docker' : 'ext-node';
    const extNodeEnv = env.getEnvVariables(extNodeEnvName);
    const dbUrl = await db.setupIsolatedDatabase(instanceName);
    for (const envVar in extNodeEnv) {
        let envVarValue = extNodeEnv[envVar];
        if (envVar === 'DATABASE_URL') {
            envVarValue = dbUrl;
        }
        envVarValue = envVarValue.replace('localhost', 'host');
        envVarValue = envVarValue.replace('127.0.0.1', 'host');
        dockerEnv += ` --env "${envVar}=${envVarValue}" `;
    }
    const artifactsHostDirectory = path.join(process.env.ZKSYNC_HOME as string, 'artifacts');
    const dockerVolumes = ` -v ${artifactsHostDirectory}:/usr/src/zksync/artifacts `;
    //http_port = 3060, ws_port = 3061, healthcheck_port = 3081, prometheus_port = 3322
    const publishedPorts = '-p 3060:3060 -p 3061:3061 -p 3081:3081 -p 3322:3322';
    const networkingFlags = '--add-host=host.docker.internal:host-gateway';
    const nameFlag = `--name ${instanceName}`;
    const allFlags = `${networkingFlags} ${publishedPorts} ${dockerVolumes} ${dockerEnv} ${nameFlag}`;
    const cmd = `docker container run -d ${allFlags} matterlabs/integration-test-rust-binaries-runner`;
    const enProcess = utils.background(cmd);

    let startTime = new Date();
    while (true) {
        if (new Date().getTime() - startTime.getTime() > 30 * 1000) {
            throw new Error('Timeout waiting for EN to start');
        }
        try {
            const healthcheckPort = extNodeEnv['EN_HEALTHCHECK_PORT'];
            const healthcheckUrl = `http://localhost:${healthcheckPort}/health`;
            const response = await fetch(healthcheckUrl);
            const json = await response.json();
            if (json['status'] == 'ready') {
                break;
            }
        } catch (err) {
            await utils.sleep(1);
            const { stdout: status } = await utils.exec(
                `docker container inspect ${instanceName} --format={{.State.Status}}`
            );
            if (status.trim() == 'exited') {
                await utils.spawn(`docker logs ${instanceName}`);
                throw new Error('EN instance exited before reporting to be ready');
            }
        }
    }
    return new IsolatedExternalNode(enProcess);
}
async function create_genesis(cmd: string) {
    await utils.confirmAction();
    await utils.spawn(`${cmd} | tee genesis.log`);
    const genesisContents = fs.readFileSync('genesis.log').toString().split('\n');
    const genesisBlockCommitment = genesisContents.find((line) => line.includes('CONTRACTS_GENESIS_BATCH_COMMITMENT='));
    const genesisBootloaderHash = genesisContents.find((line) => line.includes('CHAIN_STATE_KEEPER_BOOTLOADER_HASH='));
    const genesisDefaultAAHash = genesisContents.find((line) => line.includes('CHAIN_STATE_KEEPER_DEFAULT_AA_HASH='));
    const genesisRoot = genesisContents.find((line) => line.includes('CONTRACTS_GENESIS_ROOT='));
    const genesisRollupLeafIndex = genesisContents.find((line) =>
        line.includes('CONTRACTS_GENESIS_ROLLUP_LEAF_INDEX=')
    );
    if (genesisRoot == null || !/^CONTRACTS_GENESIS_ROOT=0x[a-fA-F0-9]{64}$/.test(genesisRoot)) {
        throw Error(`Genesis is not needed (either Postgres DB or tree's Rocks DB is not empty)`);
    }

    if (
        genesisBootloaderHash == null ||
        !/^CHAIN_STATE_KEEPER_BOOTLOADER_HASH=0x[a-fA-F0-9]{64}$/.test(genesisBootloaderHash)
    ) {
        throw Error(`Genesis is not needed (either Postgres DB or tree's Rocks DB is not empty)`);
    }

    if (
        genesisDefaultAAHash == null ||
        !/^CHAIN_STATE_KEEPER_DEFAULT_AA_HASH=0x[a-fA-F0-9]{64}$/.test(genesisDefaultAAHash)
    ) {
        throw Error(`Genesis is not needed (either Postgres DB or tree's Rocks DB is not empty)`);
    }

    if (
        genesisBlockCommitment == null ||
        !/^CONTRACTS_GENESIS_BATCH_COMMITMENT=0x[a-fA-F0-9]{64}$/.test(genesisBlockCommitment)
    ) {
        throw Error(`Genesis is not needed (either Postgres DB or tree's Rocks DB is not empty)`);
    }

    if (
        genesisRollupLeafIndex == null ||
        !/^CONTRACTS_GENESIS_ROLLUP_LEAF_INDEX=([1-9]\d*|0)$/.test(genesisRollupLeafIndex)
    ) {
        throw Error(`Genesis is not needed (either Postgres DB or tree's Rocks DB is not empty)`);
    }

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
    env.modify('CONTRACTS_GENESIS_ROOT', genesisRoot);
    env.modify('CHAIN_STATE_KEEPER_BOOTLOADER_HASH', genesisBootloaderHash);
    env.modify('CHAIN_STATE_KEEPER_DEFAULT_AA_HASH', genesisDefaultAAHash);
    env.modify('CONTRACTS_GENESIS_BATCH_COMMITMENT', genesisBlockCommitment);
    env.modify('CONTRACTS_GENESIS_ROLLUP_LEAF_INDEX', genesisRollupLeafIndex);
}

export async function genesisFromSources() {
    await create_genesis('cargo run --bin zksync_server --release -- --genesis');
}

export async function genesisFromBinary() {
    await create_genesis('zksync_server --genesis');
}

export const serverCommand = new Command('server')
    .description('start zksync server')
    .option('--genesis', 'generate genesis data via server')
    .option('--rebuild-tree', 'rebuilds merkle tree from database logs', 'rebuild_tree')
    .option('--uring', 'enables uring support for RocksDB')
    .option('--components <components>', 'comma-separated list of components to run')
    .action(async (cmd: Command) => {
        if (cmd.genesis) {
            await genesisFromSources();
        } else {
            await server(cmd.rebuildTree, cmd.uring, cmd.components);
        }
    });

export const enCommand = new Command('external-node')
    .description('start zksync external node')
    .option('--reinit', 'reset postgres and rocksdb before starting')
    .action(async (cmd: Command) => {
        await externalNode(cmd.reinit);
    });

import { Command } from 'commander';
import * as utils from './utils';
import * as env from './env';
import { clean } from './clean';
import fs from 'fs';

export async function server(rebuildTree: boolean, openzeppelinTests: boolean, components?: string) {
    let options = '';
    if (openzeppelinTests) {
        options += '--features=openzeppelin_tests';
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

async function create_genesis(cmd: string) {
    await utils.confirmAction();
    await utils.spawn(`${cmd} | tee genesis.log`);
    const genesisContents = fs.readFileSync('genesis.log').toString().split('\n');
    const genesisBlockCommitment = genesisContents.find((line) => line.includes('CONTRACTS_GENESIS_BLOCK_COMMITMENT='));
    const genesisRoot = genesisContents.find((line) => line.includes('CONTRACTS_GENESIS_ROOT='));
    const genesisRollupLeafIndex = genesisContents.find((line) =>
        line.includes('CONTRACTS_GENESIS_ROLLUP_LEAF_INDEX=')
    );
    if (genesisRoot == null || !/^CONTRACTS_GENESIS_ROOT=0x[a-fA-F0-9]{64}$/.test(genesisRoot)) {
        throw Error(`Genesis is not needed (either Postgres DB or tree's Rocks DB is not empty)`);
    }

    if (
        genesisBlockCommitment == null ||
        !/^CONTRACTS_GENESIS_BLOCK_COMMITMENT=0x[a-fA-F0-9]{64}$/.test(genesisBlockCommitment)
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
    env.modify('CONTRACTS_GENESIS_BLOCK_COMMITMENT', genesisBlockCommitment);
    env.modify('CONTRACTS_GENESIS_ROLLUP_LEAF_INDEX', genesisRollupLeafIndex);
    env.modify_contracts_toml('CONTRACTS_GENESIS_ROOT', genesisRoot);
    env.modify_contracts_toml('CONTRACTS_GENESIS_BLOCK_COMMITMENT', genesisBlockCommitment);
    env.modify_contracts_toml('CONTRACTS_GENESIS_ROLLUP_LEAF_INDEX', genesisRollupLeafIndex);
}

export async function genesis_from_sources() {
    await create_genesis('cargo run --bin zksync_server --release -- --genesis');
}

export async function genesis_from_binary() {
    await create_genesis('zksync_server --genesis');
}

export const command = new Command('server')
    .description('start zksync server')
    .option('--genesis', 'generate genesis data via server')
    .option('--rebuild-tree', 'rebuilds merkle tree from database logs', 'rebuild_tree')
    .option('--openzeppelin-tests', `enables 'openzeppelin_tests' feature`)
    .option('--components <components>', 'comma-separated list of components to run')
    .action(async (cmd: Command) => {
        if (cmd.genesis) {
            await genesis_from_sources();
        } else {
            await server(cmd.rebuildTree, cmd.openzeppelinTests, cmd.components);
        }
    });

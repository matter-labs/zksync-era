import { Command } from 'commander';
import * as fs from 'fs';
import * as path from 'path';
import { confirmAction } from './utils';
import * as down from './down';

export function clean(path: string) {
    if (fs.existsSync(path)) {
        fs.rmSync(path, { recursive: true });
        console.log(`Successfully removed ${path}`);
    }
}

export const command = new Command('clean')
    .option('--config [environment]')
    .option('--database')
    .option('--contracts')
    .option('--artifacts')
    .option('--all')
    .description('removes generated files')
    .action(async (cmd) => {
        if (!cmd.contracts && !cmd.config && !cmd.database && !cmd.backups && !cmd.artifacts) {
            cmd.all = true; // default is all
        }
        await confirmAction();

        if (cmd.all || cmd.config) {
            const envName = process.env.ZKSYNC_ENV;
            clean(`etc/env/target/${envName}.env`);
            clean(`etc/env/l2-inits/${envName}.init.env`);
        }

        if (cmd.all || cmd.artifacts) {
            clean('core/tests/ts-integration/artifacts-zk');
            clean('core/tests/ts-integration/cache-zk');
        }

        if (cmd.all || cmd.database) {
            const dbPaths = process.env.ZKSYNC_ENV?.startsWith('ext-node')
                ? [process.env.EN_MERKLE_TREE_PATH!]
                : [process.env.DATABASE_STATE_KEEPER_DB_PATH!, process.env.DATABASE_MERKLE_TREE_PATH!];
            for (const dbPath of dbPaths) {
                clean(path.dirname(dbPath));
            }
        }

        if (cmd.all || cmd.contracts) {
            clean('contracts/ethereum/artifacts');
            clean('contracts/ethereum/cache');
            clean('contracts/ethereum/typechain');
            clean('contracts/zksync/artifacts-zk');
            clean('contracts/zksync/cache-zk');
            clean('contracts/zksync/typechain');
        }

        if (cmd.all) {
            await down.down();
            clean('volumes');
            clean('contracts/ethereum/.openzeppelin');
        }
    });

import { Command } from 'commander';
import * as fs from 'fs';
import { confirmAction } from './utils';

export function clean(path: string) {
    if (fs.existsSync(path)) {
        if (fs.lstatSync(path).isDirectory()) {
            fs.rmdirSync(path, { recursive: true });
        } else {
            fs.rmSync(path);
        }
        console.log(`Successfully removed ${path}`);
    }
}

export const command = new Command('clean')
    .option('--config [environment]')
    .option('--database')
    .option('--backups')
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
            const env = process.env.ZKSYNC_ENV;
            clean(`etc/env/${env}.env`);
            clean('etc/env/.current');
            clean('etc/env/.init.env');
        }

        if (cmd.all || cmd.artifacts) {
            clean(`artifacts`);
        }

        if (cmd.all || cmd.database) {
            clean('db');
        }

        if (cmd.all || cmd.backups) {
            clean('backups');
        }

        if (cmd.all || cmd.contracts) {
            clean('contracts/ethereum/artifacts');
            clean('contracts/ethereum/cache');
            clean('contracts/ethereum/typechain');
            clean('contracts/zksync/artifacts-zk');
            clean('contracts/zksync/cache-zk');
            clean('contracts/zksync/typechain');
        }
    });

import { Command } from 'commander';
import * as fs from 'fs';
import { confirmAction } from './utils';

export function clean(directory: string) {
    if (fs.existsSync(directory)) {
        fs.rmdirSync(directory, { recursive: true });
        console.log(`Successfully removed ${directory}`);
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
        if (!cmd.contracts && !cmd.config && !cmd.database && !cmd.backups) {
            cmd.all = true; // default is all
        }
        await confirmAction();

        if (cmd.all || cmd.config) {
            const env = cmd.environment || process.env.ZKSYNC_ENV || 'dev';
            clean(`etc/env/${env}`);

            fs.rmSync(`etc/env/${env}.env`);
            console.log(`Successfully removed etc/env/${env}.env`);
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

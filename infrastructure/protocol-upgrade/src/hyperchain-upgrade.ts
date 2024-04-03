import { Command } from 'commander';
import { spawn } from 'zk/build/utils';

async function hyperchainUpgrade1() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);

    await spawn(`yarn hyperchain-upgrade-1`);
    process.chdir(cwd);
}

async function hyperchainUpgrade2() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);

    await spawn(`yarn hyperchain-upgrade-2`);
    process.chdir(cwd);
}

export const command = new Command('hyperchain-upgrade').description('create and publish custom l2 upgrade');

command
    .command('start')
    .description('start')
    .option('--phase1')
    .option('--phase2')
    .action(async (options) => {
        if (options.phase1) {
            await hyperchainUpgrade1();
        } else if (options.phase2) {
            await hyperchainUpgrade2();
        }
    });
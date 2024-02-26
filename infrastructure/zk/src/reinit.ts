import { Command } from 'commander';

import { clean } from './clean';
import * as compiler from './compiler';
import * as contract from './contract';
import * as db from './database';
import * as env from './env';
import * as run from './run/run';
import * as server from './server';
import { up } from './up';
import { announced } from './utils';

const reinitDevCmdAction = async (): Promise<void> => {
    await announced('Setting up containers', up());
    await announced('Compiling JS packages', run.yarn());
    await announced('Compile l2 contracts', compiler.compileAll());
    await announced('Drop postgres db', db.drop());
    await announced('Setup postgres db', db.setup());
    await announced('Clean rocksdb', clean(`db/${process.env.ZKSYNC_ENV!}`));
    await announced('Clean backups', clean(`backups/${process.env.ZKSYNC_ENV!}`));
    await announced('Building contracts', contract.build());
    //note no ERC20 tokens are deployed here
    await announced('Deploying L1 verifier', contract.deployVerifier());
    await announced('Reloading env', env.reload());
    await announced('Running server genesis setup', server.genesisFromSources());
    await announced('Deploying L1 contracts', contract.redeployL1());
    await announced('Deploying L2 contracts', contract.deployL2ThroughL1({ includePaymaster: true }));
    await announced('Initializing governance', contract.initializeGovernance());
};

const printReinitReadme = (): void => {
    console.log();
};

export const reinitCommand = new Command('reinit').description('').action(printReinitReadme);

reinitCommand
    .command('dev')
    .description('"Reinitializes" network. Runs faster than a full init, but requires `init dev` to be executed prior.')
    .action(reinitDevCmdAction);

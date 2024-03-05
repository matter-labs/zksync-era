import { Command, description } from 'commander';

import { clean } from './clean';
import * as compiler from './compiler';
import * as contract from './contract';
import * as db from './database';
import * as env from './env';
import * as run from './run';
import * as server from './server';
import { up } from './up';
import { announced } from './utils';
import { ADDRESS_ONE, initDatabase, initHyperchain } from './init';
import * as config from './config';

const reinitDevCmdAction = async (): Promise<void> => {
    await announced('Setting up containers', up());
    await announced('Compiling JS packages', run.yarn());
    await announced('Compile l2 contracts', compiler.compileAll());
    await announced('Drop postgres db', db.drop({ server: true, prover: true }));
    await announced('Setup postgres db', db.setup({ server: true, prover: true }));
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

const reinitHyperCmdAction = async (): Promise<void> => {
    await config.bumpChainId();
    await initDatabase({ skipVerifierDeployment: true });
    await initHyperchain({ includePaymaster: true, baseToken: { name: 'ETH', address: ADDRESS_ONE } });
};

export const reinitCommand = new Command('reinit')
    .description('"Reinitializes" network. Runs faster than a full init, but requires `init dev` to be executed prior.')
    .action(reinitDevCmdAction);

reinitCommand.command('hyper').description(' ').action(reinitHyperCmdAction);

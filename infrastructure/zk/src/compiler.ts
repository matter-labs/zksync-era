import { Command } from 'commander';
import * as utils from './utils';

export async function compileTestContracts() {
    await utils.spawn('yarn workspace contracts-test-data build');
    await utils.spawn('yarn ts-integration build');
    await utils.spawn('yarn ts-integration build-yul');
}

export async function compileSystemContracts() {
    await utils.spawn('yarn workspace zksync-erc20 build');

    process.chdir('etc/system-contracts');
    await utils.spawn('yarn');
    await utils.spawn('yarn build');
    process.chdir('../..');
}

export async function compileAll() {
    await compileSystemContracts();
    await compileTestContracts();
}

export const command = new Command('compiler').description('compile contract');

command.command('all').description('').action(compileAll);
command.command('system-contracts').description('').action(compileSystemContracts);
command.command('test-contracts').description('').action(compileTestContracts);

import { Command } from 'commander';
import * as utils from './utils';

export async function compileTestContracts() {
    await utils.spawn('yarn workspace contracts-test-data build');
    await utils.spawn('yarn ts-integration build');
    await utils.spawn('yarn ts-integration build-yul');
}

export async function compileSystemContracts() {
    await utils.spawn('yarn workspace zksync-erc20 build');

    await utils.spawn('yarn workspace system-contracts build');
}

export async function prepareCompilersCache() {
    // TODO(tomek) remove line below as soon as there is at least one pushed docker image
    await utils.spawn('zk docker build compilers');
    await utils.spawn('docker create --name temp matterlabs/compilers:latest2.0');
    await utils.spawn('mkdir -p /root/.cache/hardhat-nodejs/compilers-v2');
    await utils.spawn('docker cp temp:./.cache/hardhat-nodejs/compilers-v2/ /root/.cache/hardhat-nodejs/');
    await utils.spawn('chmod -R +x /root/.cache/hardhat-nodejs/');
    await utils.spawn('docker rm temp');
}

export async function compileAll() {
    await compileSystemContracts();
    await compileTestContracts();
}

export const command = new Command('compiler').description('compile contract');

command.command('prepare-compilers-cache').description('').action(prepareCompilersCache);
command.command('all').description('').action(compileAll);
command.command('system-contracts').description('').action(compileSystemContracts);
command.command('test-contracts').description('').action(compileTestContracts);

import { Command } from 'commander';
import { spawn } from './l1-contracts';

export async function compileTestContracts() {
    await spawn('yarn --cwd etc/contracts-test-data hardhat compile');
    process.chdir('core/tests/ts-integration');
    await spawn('yarn hardhat compile');
    await spawn('yarn hardhat run ./scripts/compile-yul.ts');
    process.chdir('../../..');
}

export async function compileSystemContracts(version: ProtocolVersions) {
    await spawn('yarn --cwd etc/ERC20 hardhat compile');

    process.chdir(systemContractsPathForProtocol(version));
    await spawn('yarn');
    await spawn('yarn hardhat compile');
    await spawn('yarn preprocess');
    await spawn('yarn hardhat run ./scripts/compile-yul.ts');
    process.chdir('../..');
}

export async function compileAll(version: ProtocolVersions) {
    await compileSystemContracts(version);
    await compileTestContracts();
}

export const command = new Command('system-contracts').description('compile system contracts');

command
    .command('all')
    .option('--protocol-version', 'Protocol version for contracts')
    .description('')
    .action(compileAll);
command.command('system-contracts').description('').action(compileSystemContracts);
command.command('test-contracts').description('').action(compileTestContracts);

import {Command, program} from 'commander';
import {
    build,
    deployL1,
    initializeL1AllowList,
    initializeValidator,
    redeployL1,
    verifyL1Contracts
} from './l1-contracts';
import { deployL2 } from './l2-contracts';
import * as env from "zk/build/env";
import {spawnSync} from "child_process";

export const command = new Command('contract').option('--protocol-version').description('contract management');

command
    .command('redeploy [deploy-opts...]')
    .allowUnknownOption(true)
    .description('redeploy contracts')
    .action(redeployL1);
command.command('deploy [deploy-opts...]').allowUnknownOption(true).description('deploy contracts').action(deployL1);
command.command('build').description('build contracts').action(build);
command.command('initialize-validator').description('initialize validator').action(initializeValidator);
command
    .command('initialize-l1-allow-list-contract')
    .description('initialize L1 allow list contract')
    .action(initializeL1AllowList);
command.command('verify').description('verify L1 contracts').action(verifyL1Contracts);
command.command('deployl2').description('deploy l2 contacts').action(deployL2);


async function main() {
    const cwd = process.cwd();
    const ZKSYNC_HOME = process.env.ZKSYNC_HOME;

    if (!ZKSYNC_HOME) {
        throw new Error('Please set $ZKSYNC_HOME to the root of zkSync repo!');
    } else {
        process.chdir(ZKSYNC_HOME);
    }

    env.load();

    program.version('0.1.0').name('zk').description('zksync workflow tools');

    // for (const command of COMMANDS) {
        program.addCommand(command);
    // }

    // f command is special-cased because it is necessary
    // for it to run from $PWD and not from $ZKSYNC_HOME
    program
        .command('f <command...>')
        .allowUnknownOption()
        .action((command: string[]) => {
            process.chdir(cwd);
            const result = spawnSync(command[0], command.slice(1), { stdio: 'inherit' });
            if (result.error) {
                throw result.error;
            }
            process.exitCode = result.status || undefined;
        });

    await program.parseAsync(process.argv);
}

main().catch((err: Error) => {
    console.error('Error:', err.message || err);
    process.exitCode = 1;
});

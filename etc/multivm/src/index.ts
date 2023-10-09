import {Command} from "commander";
import {
    build,
    deployL1,
    initializeL1AllowList,
    initializeValidator,
    redeployL1,
    verifyL1Contracts
} from "./l1-contracts";
import {deployL2} from "./l2-contracts";

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

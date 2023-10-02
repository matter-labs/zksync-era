import fs from 'fs';
import { Command } from 'commander';
import {
    DEFAULT_L1CONTRACTS_FOR_UPGRADE_PATH,
    DEFAULT_L2CONTRACTS_FOR_UPGRADE_PATH,
    getNameOfTheLastUpgrade
} from './utils';

async function createCustomUpgrade(defaultUpgradePath: string) {
    const name = getNameOfTheLastUpgrade();
    const nameWithoutTimestamp = name.split('-').slice(1).join('_');
    const upgradePath = `${defaultUpgradePath}/${name}`;
    const upgradeFile = `${upgradePath}/Upgrade.sol`;
    fs.mkdirSync(upgradePath, { recursive: true });
    fs.copyFileSync(`${defaultUpgradePath}/DefaultUpgrade.sol`, upgradeFile);
    let result = fs.readFileSync(upgradeFile, 'utf8').replace('DefaultUpgrade', nameWithoutTimestamp);
    fs.writeFileSync(upgradeFile, result, 'utf8');
    console.log(`Custom upgrade ${name} created in ${upgradePath}`);
}

export const command = new Command('custom-upgrade').description('create and publish custom l2 upgrade');

command
    .command('create')
    .option('--l2')
    .option('--l1')
    .description('Create custom contract upgrade')
    .action(async (options) => {
        if (options.l2) {
            await createCustomUpgrade(DEFAULT_L2CONTRACTS_FOR_UPGRADE_PATH);
        } else if (options.l1) {
            await createCustomUpgrade(DEFAULT_L1CONTRACTS_FOR_UPGRADE_PATH);
        } else {
            throw new Error('Please specify --l1 or --l2');
        }
    });

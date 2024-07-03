import fs from 'fs';
import { Command } from 'commander';
import { DEFAULT_UPGRADE_PATH, getNameOfTheLastUpgrade, getTimestampInSeconds } from './utils';

function createNewUpgrade(name, protocolVersion: string) {
    const timestamp = getTimestampInSeconds();
    const upgradePath = `${DEFAULT_UPGRADE_PATH}/${timestamp}-${name}`;
    fs.mkdirSync(upgradePath, { recursive: true });
    const upgradeJson = {
        name,
        creationTimestamp: timestamp,
        protocolVersion
    };
    fs.writeFileSync(`${upgradePath}/common.json`, JSON.stringify(upgradeJson, null, 2));
    console.log(`Created new upgrade: ${upgradePath}`);
}

export const command = new Command('upgrades').description('manage protocol upgrades');

command
    .command('create <name>')
    .requiredOption('--protocol-version <protocolVersion>')
    .description('Create new upgrade')
    .action(async (name, cmd) => {
        createNewUpgrade(name, cmd.protocolVersion);
    });

command
    .command('get-last')
    .description('get name of the last upgrade')
    .action(async () => {
        console.log(getNameOfTheLastUpgrade());
    });

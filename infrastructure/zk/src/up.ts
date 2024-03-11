import { Command } from 'commander';
import * as utils from './utils';
import { down } from './down';
import fs from 'fs';

// Make sure that the volumes exists before starting the containers.
export function createVolumes() {
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/volumes/reth/data`, { recursive: true });
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/volumes/reth/logs`, { recursive: true });
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/volumes/postgres`, { recursive: true });
}

export async function up(composeFile?: string) {
    await utils.sleep(1);
    // createVolumes();
    if (composeFile) {
        await utils.spawn(`docker compose -f ${composeFile} up -d reth postgres`);
    } else {
        await utils.spawn('docker compose up -d');
    }
}

export const command = new Command('up')
    .description('start development containers')
    .option('--docker-file <dockerFile>', 'path to a custom docker file')
    .action(async (cmd) => {
        await up(cmd.dockerFile);
    });

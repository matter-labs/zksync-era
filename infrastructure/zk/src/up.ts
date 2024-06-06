import { Command } from 'commander';
import * as utils from 'utils';
import fs from 'fs';

// Make sure that the volumes exists before starting the containers.
export function createVolumes() {
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/volumes/reth/data`, {
        recursive: true
    });
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/volumes/postgres`, {
        recursive: true
    });
}

export async function up(runObservability: boolean, composeFile?: string) {
    if (composeFile) {
        await utils.spawn(`docker compose -f ${composeFile} up -d`);
    } else {
        await utils.spawn('docker compose up -d');
    }
    if (runObservability) {
        await utils.spawn(`docker compose -f ./target/dockprom/docker-compose.yml up -d`);
    }
}

export const command = new Command('up')
    .description('start development containers')
    .option('--docker-file <dockerFile>', 'path to a custom docker file')
    .option('--run-observability', 'whether to run observability stack')
    .action(async (cmd) => {
        await up(cmd.runObservability, cmd.dockerFile);
    });

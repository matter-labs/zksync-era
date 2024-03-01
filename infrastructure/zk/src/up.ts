import { Command } from 'commander';
import * as utils from './utils';
import { down } from './down';
import fs from 'fs';

// Make sure that the volumes exists before starting the containers.
export function createVolumes() {
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/volumes/geth`, { recursive: true });
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/volumes/prysm/beacon`, { recursive: true });
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/volumes/prysm/validator`, { recursive: true });
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/volumes/postgres`, { recursive: true });

    fs.copyFileSync(
        `${process.env.ZKSYNC_HOME}/docker/prysm/config.yml`,
        `${process.env.ZKSYNC_HOME}/volumes/prysm/config.yml`
    );

    fs.copyFileSync(
        `${process.env.ZKSYNC_HOME}/docker/geth/jwtsecret`,
        `${process.env.ZKSYNC_HOME}/volumes/geth/jwtsecret`
    );
    fs.copyFileSync(
        `${process.env.ZKSYNC_HOME}/docker/geth/password.sec`,
        `${process.env.ZKSYNC_HOME}/volumes/geth/password.sec`
    );
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/volumes/geth/keystore`, { recursive: true });
    fs.copyFileSync(
        `${process.env.ZKSYNC_HOME}/docker/geth/keystore/UTC--2019-04-06T21-13-27.692266000Z--8a91dc2d28b689474298d91899f0c1baf62cb85b`,
        `${process.env.ZKSYNC_HOME}/volumes/geth/keystore/UTC--2019-04-06T21-13-27.692266000Z--8a91dc2d28b689474298d91899f0c1baf62cb85b`
    );
}

export async function up(composeFile?: string) {
    await down();
    // There is some race on the filesystem, so backoff here
    await utils.sleep(1);
    createVolumes();
    if (composeFile) {
        await utils.spawn(`docker compose -f ${composeFile} up -d geth postgres`);
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

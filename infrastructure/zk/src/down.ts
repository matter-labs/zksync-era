import { Command } from 'commander';
import * as utils from './utils';

export async function down() {
    await utils.spawn('docker compose down -v');
    await utils.spawn('docker compose rm -s -f -v');
    await utils.spawn('docker run --rm -v ./volumes:/volumes postgres:14 bash -c "rm -rf /volumes/*"');
}

export const command = new Command('down').description('stop development containers').action(down);

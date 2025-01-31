import { Command } from 'commander';
import * as utils from 'utils';
import * as fs from 'fs';

export async function down() {
    await utils.spawn('docker compose down -v');
    await utils.spawn('docker compose rm -s -f -v');
    await utils.spawn('docker run --rm -v ./volumes:/volumes postgres:14 bash -c "rm -rf /volumes/*"');
    // cleaning up dockprom
    // no need to delete the folder - it's going to be deleted on the next start
    if (fs.existsSync('./target/dockprom/docker-compose.yml')) {
        await utils.spawn('docker compose -f ./target/dockprom/docker-compose.yml down -v');
    }
}

export const command = new Command('down').description('stop development containers').action(down);

import { Command } from 'commander';
import * as utils from './utils';

export async function reset() {
    await utils.confirmAction();
    await wait();
    await drop();
    await setup();
}

export async function resetTest() {
    const databaseUrl = process.env.DATABASE_URL as string;
    process.env.DATABASE_URL = databaseUrl.replace('zksync_local', 'zksync_local_test');
    await utils.confirmAction();
    await drop();
    await setup();
}

export async function drop() {
    await utils.confirmAction();
    console.log('Dropping DB...');
    await utils.spawn('cargo sqlx database drop -y');
}

export async function migrate() {
    await utils.confirmAction();
    console.log('Running migrations...');
    await utils.spawn('cd core/lib/dal && cargo sqlx database create && cargo sqlx migrate run');
}

export async function generateMigration(name: String) {
    console.log('Generating migration... ');
    process.chdir('core/lib/dal');
    await utils.exec(`cargo sqlx migrate add -r ${name}`);

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function setup() {
    process.chdir('core/lib/dal');
    const localDbUrl = 'postgres://postgres@localhost';
    if (process.env.DATABASE_URL!.startsWith(localDbUrl)) {
        console.log(`Using localhost database:`);
        console.log(`DATABASE_URL = ${process.env.DATABASE_URL}`);
    } else {
        // Remote database, we can't show the contents.
        console.log(`WARNING! Using prod db!`);
    }
    await utils.spawn('cargo sqlx database create');
    await utils.spawn('cargo sqlx migrate run');
    if (process.env.DATABASE_URL!.startsWith(localDbUrl)) {
        await utils.spawn('cargo sqlx prepare --check -- --tests || cargo sqlx prepare -- --tests');
    }

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function wait(tries: number = 4) {
    for (let i = 0; i < tries; i++) {
        const result = await utils.allowFail(utils.exec(`pg_isready -d "${process.env.DATABASE_URL}"`));
        if (result !== null) return; // null means failure
        await utils.sleep(5);
    }
    await utils.exec(`pg_isready -d "${process.env.DATABASE_URL}"`);
}

export async function checkSqlxData() {
    process.chdir('core/lib/dal');
    await utils.spawn('cargo sqlx prepare --check -- --tests');
    process.chdir(process.env.ZKSYNC_HOME as string);
}

export const command = new Command('db').description('database management');

command.command('drop').description('drop the database').action(drop);
command.command('migrate').description('run migrations').action(migrate);
command.command('new-migration <name>').description('generate a new migration').action(generateMigration);
command.command('setup').description('initialize the database and perform migrations').action(setup);
command.command('wait').description('wait for database to get ready for interaction').action(wait);
command.command('reset').description('reinitialize the database').action(reset);
command.command('reset-test').description('reinitialize the database for test').action(resetTest);
command.command('check-sqlx-data').description('check sqlx-data.json is up to date').action(checkSqlxData);

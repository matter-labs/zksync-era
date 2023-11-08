import { Command } from 'commander';
import * as utils from './utils';

export async function resetMain() {
    await utils.confirmAction();
    await waitMain();
    await dropMain();
    await setupMain();
}

export async function resetTest() {
    process.env.DATABASE_URL = process.env.TEST_DATABASE_URL;
    await utils.confirmAction();

    await waitMain(100);

    console.log('setting up a database template');
    await setupMain();

    console.log('disallowing connections to the template');
    await utils.spawn(
        `psql "${process.env.DATABASE_URL}" -c "update pg_database set datallowconn = false where datname = current_database()"`
    );
}

export async function dropMain() {
    await utils.confirmAction();
    console.log('Dropping main DB...');
    await utils.spawn(`cargo sqlx database drop --database-url ${process.env.DATABASE_URL}`);
}

export async function migrateMain() {
    await utils.confirmAction();
    console.log('Running main migrations...');
    await utils.spawn(
        `cd core/lib/server_dal && cargo sqlx database create --database-url ${process.env.DATABASE_URL} && cargo sqlx migrate run --database-url ${process.env.DATABASE_URL}`
    );
}

export async function generateMigrationMain(name: String) {
    console.log('Generating main migration... ');
    process.chdir('core/lib/server_dal');
    await utils.exec(`cargo sqlx migrate add -r ${name}`);

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function setupMain() {
    process.chdir('core/lib/server_dal');
    const localDbUrl = 'postgres://postgres@localhost';
    if (process.env.DATABASE_URL!.startsWith(localDbUrl)) {
        console.log(`Using localhost database:`);
        console.log(`DATABASE_URL = ${process.env.DATABASE_URL}`);
    } else {
        // Remote database, we can't show the contents.
        console.log(`WARNING! Using prod main db!`);
    }
    await utils.spawn(`cargo sqlx database create --database-url ${process.env.DATABASE_URL}`);
    await utils.spawn(`cargo sqlx migrate run --database-url ${process.env.DATABASE_URL}`);
    if (process.env.DATABASE_URL!.startsWith(localDbUrl)) {
        await utils.spawn(
            `cargo sqlx prepare --check --database-url ${process.env.DATABASE_URL} -- --tests || cargo sqlx prepare --database-url ${process.env.DATABASE_URL} -- --tests`
        );
    }

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function waitMain(tries: number = 4) {
    for (let i = 0; i < tries; i++) {
        const result = await utils.allowFail(utils.exec(`pg_isready -d "${process.env.DATABASE_URL}"`));
        if (result !== null) return; // null means failure
        console.log(`waiting for postgres ${process.env.DATABASE_URL}`);
        await utils.sleep(1);
    }
    await utils.exec(`pg_isready -d "${process.env.DATABASE_URL}"`);
}

export async function checkSqlxDataMain() {
    process.chdir('core/lib/server_dal');
    await utils.spawn(`cargo sqlx prepare --check --database-url ${process.env.DATABASE_URL} -- --tests`);
    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function resetProver() {
    await utils.confirmAction();
    await waitProver();
    await dropProver();
    await setupProver();
}

export async function resetTestProver() {
    process.env.DATABASE_PROVER_URL = process.env.TEST_DATABASE_PROVER_URL;
    await utils.confirmAction();

    // console.log('recreating postgres container for unit tests');
    // await utils.spawn('docker compose -f docker-compose-unit-tests.yml down');
    // await utils.spawn('docker compose -f docker-compose-unit-tests.yml up -d');

    await waitProver(100);

    console.log('setting up a prover database template');
    await setupProver();

    console.log('disallowing connections to the template');
    await utils.spawn(
        `psql "${process.env.DATABASE_PROVER_URL}" -c "update pg_database set datallowconn = false where datname = current_database()"`
    );
}

export async function dropProver() {
    await utils.confirmAction();
    console.log('Dropping prover DB...');
    await utils.spawn(`cargo sqlx database drop --database-url ${process.env.DATABASE_PROVER_URL}`);
}

export async function migrateProver() {
    await utils.confirmAction();
    console.log('Running prover migrations...');
    await utils.spawn(
        `cd prover/prover_dal && cargo sqlx database create --database-url ${process.env.DATABASE_PROVER_URL} && cargo sqlx migrate run --database-url ${process.env.DATABASE_PROVER_URL}`
    );
}

export async function generateMigrationProver(name: String) {
    console.log('Generating prover migration... ');
    process.chdir('prover/prover_dal');
    await utils.exec(`cargo sqlx migrate add -r ${name}`);

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function setupProver() {
    process.chdir('prover/prover_dal');
    const localDbUrl = 'postgres://postgres@localhost';
    if (process.env.DATABASE_PROVER_URL!.startsWith(localDbUrl)) {
        console.log(`Using localhost prover database:`);
        console.log(`DATABASE_PROVER_URL = ${process.env.DATABASE_PROVER_URL}`);
    } else {
        // Remote database, we can't show the contents.
        console.log(`WARNING! Using prod prover db!`);
    }
    await utils.spawn(`cargo sqlx database create --database-url ${process.env.DATABASE_PROVER_URL}`);
    await utils.spawn(`cargo sqlx migrate run --database-url ${process.env.DATABASE_PROVER_URL}`);
    if (process.env.DATABASE_PROVER_URL!.startsWith(localDbUrl)) {
        await utils.spawn(
            `cargo sqlx prepare --check --database-url ${process.env.DATABASE_PROVER_URL} -- --tests || cargo sqlx prepare --database-url ${process.env.DATABASE_PROVER_URL} -- --tests`
        );
    }

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function waitProver(tries: number = 4) {
    for (let i = 0; i < tries; i++) {
        const result = await utils.allowFail(utils.exec(`pg_isready -d '${process.env.DATABASE_PROVER_URL}'`));
        if (result !== null) return; // null means failure
        console.log(`waiting for postgres ${process.env.DATABASE_PROVER_URL}`);
        await utils.sleep(1);
    }
    await utils.exec(`pg_isready -d '${process.env.DATABASE_PROVER_URL}'`);
}

export async function checkSqlxDataProver() {
    process.chdir('prover/prover_dal');
    await utils.spawn(`cargo sqlx prepare --check --database-url ${process.env.DATABASE_PROVER_URL} -- --tests`);
    process.chdir(process.env.ZKSYNC_HOME as string);
}

export const command = new Command('db').description('database management');

command.command('drop').description('drop the database').action(dropMain);
command.command('migrate').description('run migrations').action(migrateMain);
command.command('new-migration <name>').description('generate a new migration').action(generateMigrationMain);
command.command('setup').description('initialize the database and perform migrations').action(setupMain);
command.command('wait').description('wait for database to get ready for interaction').action(waitMain);
command.command('reset').description('reinitialize the database').action(resetMain);
command.command('reset-test').description('reinitialize the database for test').action(resetTest);
command.command('check-sqlx-data').description('check sqlx-data.json is up to date').action(checkSqlxDataMain);

command.command('prover-drop').description('drop the prover database').action(dropProver);
command.command('prover-migrate').description('run prover migrations').action(migrateProver);
command
    .command('prover-new-migration <name>')
    .description('generate a new migration for prover db')
    .action(generateMigrationProver);
command
    .command('prover-setup')
    .description('initialize the prover database and perform migrations')
    .action(setupProver);
command.command('prover-wait').description('wait for prover database to get ready for interaction').action(waitProver);
command.command('prover-reset').description('reinitialize the prover database').action(resetProver);
command.command('prover-reset-test').description('reinitialize the prover database for test').action(resetTestProver);
command
    .command('prover-check-sqlx-data')
    .description('check prover sqlx-data.json is up to date')
    .action(checkSqlxDataProver);

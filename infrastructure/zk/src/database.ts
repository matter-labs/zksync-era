import { Command } from 'commander';
import * as utils from './utils';

export async function wait(database_url: string, tries: number = 4) {
    for (let i = 0; i < tries; i++) {
        const result = await utils.allowFail(utils.exec(`pg_isready -d "${database_url}"`));
        if (result !== null) return; // null means failure
        console.log(`waiting for postgres ${database_url}`);
        await utils.sleep(1);
    }
    await utils.exec(`pg_isready -d "${database_url}"`);
}

export async function drop(database_url: string) {
    await utils.confirmAction();
    console.log('Dropping DB...');
    await utils.spawn(`cargo sqlx database drop --database-url ${database_url}`);
}

export async function setup(path: string, database_url: string) {
    process.chdir(path);
    const localDbUrl = 'postgres://postgres@localhost';
    if (database_url.startsWith(localDbUrl)) {
        console.log(`Using localhost database:`);
        console.log(`DATABASE_URL = ${database_url}`);
    } else {
        // Remote database, we can't show the contents.
        console.log(`WARNING! Using prod main db!`);
    }
    await utils.spawn(`cargo sqlx database create --database-url ${database_url}`);
    await utils.spawn(`cargo sqlx migrate run --database-url ${database_url}`);
    if (database_url!.startsWith(localDbUrl)) {
        await utils.spawn(
            `cargo sqlx prepare --check --database-url ${database_url} -- --tests || cargo sqlx prepare --database-url ${database_url} -- --tests`
        );
    }

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function resetTest(path: string, test_database_url: string) {
    await utils.confirmAction();

    await wait(test_database_url, 100);

    console.log('setting up a database template');
    await setup(path, test_database_url);

    console.log('disallowing connections to the template');
    await utils.spawn(
        `psql "${test_database_url}" -c "update pg_database set datallowconn = false where datname = current_database()"`
    );
}

export async function migrate(path: string, database_url: string) {
    await utils.confirmAction();
    console.log('Running migrations...');
    await utils.spawn(
        `cd ${path} && cargo sqlx database create --database-url ${database_url} && cargo sqlx migrate run --database-url ${database_url}`
    );
}

export async function generateMigration(name: String, path: string) {
    console.log('Generating migration...');
    process.chdir(path);
    await utils.exec(`cargo sqlx migrate add -r ${name}`);

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function checkSqlxData(path: string, database_url: string) {
    process.chdir(path);
    await utils.spawn(`cargo sqlx prepare --check --database-url ${database_url} -- --tests`);
    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function resetMain() {
    await utils.confirmAction();
    let db_url = process.env.DATABASE_URL!;
    await wait(db_url);
    await drop(db_url);
    await setup('core/lib/server_dal', db_url);
}

export async function dropMain() {
    await drop(process.env.DATABASE_URL!);
}

export async function migrateMain() {
    await migrate('core/lib/server_dal', process.env.DATABASE_URL!);
}

export async function generateMigrationMain(name: string) {
    await generateMigration(name, 'core/lib/server_dal');
}

export async function setupMain() {
    await setup('core/lib/server_dal', process.env.DATABASE_URL!);
}

export async function waitMain() {
    await wait(process.env.DATABASE_URL!);
}
export async function checkSqlxDataMain() {
    await checkSqlxData('core/lib/server_dal', process.env.DATABASE_URL!);
}

export async function resetTestMain() {
    await resetTest('core/lib/server_dal', process.env.TEST_DATABASE_URL!);
}

export async function resetProver() {
    await utils.confirmAction();
    let db_url = process.env.DATABASE_PROVER_URL!;
    await wait(db_url);
    await drop(db_url);
    await setup('prover/prover_dal', db_url);
}

export async function dropProver() {
    await drop(process.env.DATABASE_PROVER_URL!);
}

export async function migrateProver() {
    await migrate('prover/prover_dal', process.env.DATABASE_PROVER_URL!);
}

export async function generateMigrationProver(name: string) {
    await generateMigration(name, 'prover/prover_dal');
}

export async function setupProver() {
    await setup('prover/prover_dal', process.env.DATABASE_PROVER_URL!);
}

export async function waitProver() {
    await wait(process.env.DATABASE_PROVER_URL!);
}
export async function checkSqlxDataProver() {
    await checkSqlxData('prover/prover_dal', process.env.DATABASE_PROVER_URL!);
}

export async function resetTestProver() {
    await resetTest('prover/prover_dal', process.env.TEST_DATABASE_PROVER_URL!);
}

export const command = new Command('db').description('database management');

command.command('server-drop').description('drop the database').action(dropMain);
command.command('server-migrate').description('run migrations').action(migrateMain);
command.command('server-new-migration <name>').description('generate a new migration').action(generateMigrationMain);
command.command('server-setup').description('initialize the database and perform migrations').action(setupMain);
command.command('server-wait').description('wait for database to get ready for interaction').action(waitMain);
command.command('server-reset').description('reinitialize the database').action(resetMain);
command.command('server-reset-test').description('reinitialize the database for test').action(resetTestMain);
command.command('server-check-sqlx-data').description('check sqlx-data.json is up to date').action(checkSqlxDataMain);

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

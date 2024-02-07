import { Command } from 'commander';
import * as utils from './utils';

export async function reset(opts: any) {
    await utils.confirmAction();
    await wait(opts);
    await drop(opts);
    await setup(opts);
}

function getDals(opts: any): Map<string, string> {
    let dals = new Map<string, string>();
    if (!opts.prover && !opts.server) {
        dals.set('core/lib/dal', process.env.DATABASE_URL!);
        dals.set('prover/prover_dal', process.env.DATABASE_PROVER_URL!);
    }
    if (opts.prover) {
        dals.set('prover/prover_dal', process.env.DATABASE_PROVER_URL!);
    }
    if (opts.server) {
        dals.set('core/lib/dal', process.env.DATABASE_URL!);
    }
    return dals;
}

function getTestDals(opts: any): Map<string, string> {
    let dals = new Map<string, string>();
    if (!opts.prover && !opts.server) {
        dals.set('core/lib/dal', process.env.DATABASE_TEST_URL!);
        dals.set('prover/prover_dal', process.env.DATABASE_PROVER_URL!);
    }
    if (opts.prover) {
        dals.set('prover/prover_dal', process.env.DATABASE_PROVER_TEST_URL!);
    }
    if (opts.server) {
        dals.set('core/lib/dal', process.env.DATABASE_TEST_URL!);
    }
    return dals;
}

async function resetTestDal(dalPath: string, dalDb: string) {
    console.log('recreating postgres container for unit tests');
    await utils.spawn('docker compose -f docker-compose-unit-tests.yml down');
    await utils.spawn('docker compose -f docker-compose-unit-tests.yml up -d');
    await wait(100);
    console.log('setting up a database template');
    await setupForDal(dalPath, dalDb);
    console.log('disallowing connections to the template');
    await utils.spawn(
        `psql "${dalDb}" -c "update pg_database set datallowconn = false where datname = current_database()"`
    );
}

export async function resetTest(opts: any) {
    await utils.confirmAction();
    let dals = getTestDals(opts);
    for (const [dalPath, dalDb] of dals.entries()) {
        await resetTestDal(dalPath, dalDb);
    }
}

async function dropForDal(dalPath: string, dalDb: string) {
    console.log(`Dropping DB for dal ${dalPath}...`);
    await utils.spawn(`cargo sqlx database drop -y --database-url ${dalDb}`);
}

export async function drop(opts: any) {
    await utils.confirmAction();
    let dals = getDals(opts);
    for (const [dalPath, dalDb] of dals.entries()) {
        await dropForDal(dalPath, dalDb);
    }
}

async function migrateForDal(dalPath: string, dalDb: string) {
    console.log(`Running migrations for ${dalPath}...`);
    await utils.spawn(
        `cd ${dalPath} && cargo sqlx database create --database-url ${dalDb} && cargo sqlx migrate run --database-url ${dalDb}`
    );
}

export async function migrate(opts: any) {
    await utils.confirmAction();
    let dals = getDals(opts);
    for (const [dalPath, dalDb] of dals.entries()) {
        await migrateForDal(dalPath, dalDb);
    }
}

async function generateMigrationForDal(dalPath: string, dalDb: string, name: String) {
    console.log(`Generating migration for ${dalPath}...`);
    await utils.spawn(`cd ${dalPath} && cargo sqlx migrate add -r ${name}`);
}

export async function generateMigration(opts: any, name: String) {
    console.log('Generating migration... ');
    let dals = getDals(opts);
    for (const [dalPath, dalDb] of dals.entries()) {
        await generateMigrationForDal(dalPath, dalDb, name);
    }

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function setupForDal(dalPath: string, dalDb: string) {
    process.chdir(dalPath);
    const localDbUrl = 'postgres://postgres@localhost';
    if (dalDb.startsWith(localDbUrl)) {
        console.log(`Using localhost database -- ${dalDb}`);
    } else {
        // Remote database, we can't show the contents.
        console.log(`WARNING! Using prod main db!`);
    }
    await utils.spawn(`cargo sqlx database create --database-url ${dalDb}`);
    await utils.spawn(`cargo sqlx migrate run --database-url ${dalDb}`);
    if (dalDb.startsWith(localDbUrl)) {
        await utils.spawn(
            `cargo sqlx prepare --check --database-url ${dalDb} -- --tests || cargo sqlx prepare --database-url ${dalDb} -- --tests`
        );
    }

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function setup(opts: any) {
    let dals = getDals(opts);
    for (const [dalPath, dalDb] of dals.entries()) {
        await setupForDal(dalPath, dalDb);
    }
}

async function waitForDal(dalDb: string, tries: number) {
    for (let i = 0; i < tries; i++) {
        const result = await utils.allowFail(utils.exec(`pg_isready -d "${dalDb}"`));
        if (result !== null) return; // null means failure
        console.log(`waiting for postgres ${dalDb}`);
        await utils.sleep(1);
    }
    await utils.exec(`pg_isready -d "${dalDb}"`);
}

export async function wait(opts: any, tries: number = 4) {
    let dals = getDals(opts);
    for (const dalDb of dals.values()) {
        await waitForDal(dalDb, tries);
    }
}

async function checkSqlxDataForDal(dalPath: string, dalDb: string) {
    process.chdir(dalPath);
    await utils.spawn(`cargo sqlx prepare --check -- --tests --database-url ${dalDb}`);
    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function checkSqlxData(opts: any) {
    let dals = getDals(opts);
    for (const [dalPath, dalDb] of dals.entries()) {
        await checkSqlxDataForDal(dalPath, dalDb);
    }
}

export const command = new Command('db').description('database management');

command.command('drop').description('drop the database').option('-p, --prover').option('-s, --server').action(drop);
command.command('migrate').description('run migrations').option('-p, --prover').option('-s, --server').action(migrate);
command
    .command('new-migration <name>')
    .description('generate a new migration')
    .option('-p, --prover')
    .option('-s, --server')
    .action(generateMigration);
command
    .command('setup')
    .description('initialize the database and perform migrations')
    .option('-p, --prover')
    .option('-s, --server')
    .action(setup);
command
    .command('wait')
    .description('wait for database to get ready for interaction')
    .option('-p, --prover')
    .option('-s, --server')
    .action(wait);
command
    .command('reset')
    .description('reinitialize the database')
    .option('-p, --prover')
    .option('-s, --server')
    .action(reset);
command
    .command('reset-test')
    .description('reinitialize the database for test')
    .option('-p, --prover')
    .option('-s, --server')
    .action(resetTest);
command
    .command('check-sqlx-data')
    .description('check sqlx-data.json is up to date')
    .option('-p, --prover')
    .option('-s, --server')
    .action(checkSqlxData);

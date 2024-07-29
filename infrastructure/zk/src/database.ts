import { Command } from 'commander';
import * as utils from 'utils';

export async function reset(opts: DbOpts) {
    await utils.confirmAction();
    await wait(opts);
    await drop(opts);
    await setup(opts, false);
}

export enum DalPath {
    CoreDal = 'core/lib/dal',
    ProverDal = 'prover/prover_dal'
}

export interface DbOpts {
    core: boolean;
    prover: boolean;
}

function getDals(opts: DbOpts): Map<DalPath, string> {
    let dals = new Map<DalPath, string>();
    if (!opts.prover && !opts.core) {
        dals.set(DalPath.CoreDal, process.env.DATABASE_URL!);
        if (process.env.DATABASE_PROVER_URL) {
            dals.set(DalPath.ProverDal, process.env.DATABASE_PROVER_URL);
        }
    }
    if (opts.prover && process.env.DATABASE_PROVER_URL) {
        dals.set(DalPath.ProverDal, process.env.DATABASE_PROVER_URL);
    }
    if (opts.core) {
        dals.set(DalPath.CoreDal, process.env.DATABASE_URL!);
    }
    return dals;
}

function getTestDals(opts: DbOpts): Map<DalPath, string> {
    let dals = new Map<DalPath, string>();
    if (!opts.prover && !opts.core) {
        dals.set(DalPath.CoreDal, process.env.TEST_DATABASE_URL!);
        dals.set(DalPath.ProverDal, process.env.TEST_DATABASE_PROVER_URL!);
    }
    if (opts.prover) {
        dals.set(DalPath.ProverDal, process.env.TEST_DATABASE_PROVER_URL!);
    }
    if (opts.core) {
        dals.set(DalPath.CoreDal, process.env.TEST_DATABASE_URL!);
    }
    return dals;
}

async function resetTestDal(dalPath: DalPath, dbUrl: string) {
    console.log('recreating postgres container for unit tests');
    await utils.spawn('docker compose -f docker-compose-unit-tests.yml down');
    await utils.spawn('docker compose -f docker-compose-unit-tests.yml up -d');
    await waitForDal(dbUrl, 100);
    console.log('setting up a database template');
    await setupForDal(dalPath, dbUrl, true);
    console.log('disallowing connections to the template');
    await utils.spawn(
        `psql "${dbUrl}" -c "update pg_database set datallowconn = false where datname = current_database()"`
    );
}

export async function resetTest(opts: DbOpts) {
    await utils.confirmAction();
    let dals = getTestDals(opts);
    for (const [dalPath, dbUrl] of dals.entries()) {
        await resetTestDal(dalPath, dbUrl);
    }
}

async function dropForDal(dalPath: DalPath, dbUrl: string) {
    console.log(`Dropping DB for dal ${dalPath}...`);
    await utils.spawn(`cargo sqlx database drop -y --database-url ${dbUrl}`);
}

export async function drop(opts: DbOpts) {
    await utils.confirmAction();
    let dals = getDals(opts);
    for (const [dalPath, dbUrl] of dals.entries()) {
        await dropForDal(dalPath, dbUrl);
    }
}

async function migrateForDal(dalPath: DalPath, dbUrl: string) {
    console.log(`Running migrations for ${dalPath}...`);
    await utils.spawn(
        `cd ${dalPath} && cargo sqlx database create --database-url ${dbUrl} && cargo sqlx migrate run --database-url ${dbUrl}`
    );
}

export async function migrate(opts: DbOpts) {
    await utils.confirmAction();
    let dals = getDals(opts);
    for (const [dalPath, dbUrl] of dals.entries()) {
        await migrateForDal(dalPath, dbUrl);
    }
}

async function generateMigrationForDal(dalPath: DalPath, dbUrl: string, name: String) {
    console.log(`Generating migration for ${dalPath}...`);
    await utils.spawn(`cd ${dalPath} && cargo sqlx migrate add -r ${name}`);
}

export enum DbType {
    Core,
    Prover
}

export async function generateMigration(dbType: DbType, name: string) {
    console.log('Generating migration... ');
    if (dbType === DbType.Core) {
        await generateMigrationForDal(DalPath.CoreDal, process.env.DATABASE_URL!, name);
    } else if (dbType === DbType.Prover) {
        await generateMigrationForDal(DalPath.ProverDal, process.env.DATABASE_PROVER_URL!, name);
    }
    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function setupForDal(dalPath: DalPath, dbUrl: string, skipDbPreparation: boolean) {
    process.chdir(dalPath);
    const localDbUrl = 'postgres://postgres:notsecurepassword@localhost';
    if (dbUrl.startsWith(localDbUrl)) {
        console.log(`Using localhost database -- ${dbUrl}`);
    } else {
        // Remote database, we can't show the contents.
        console.log(`WARNING! Using prod db!`);
    }
    await utils.spawn(`cargo sqlx database create --database-url ${dbUrl}`);
    await utils.spawn(`cargo sqlx migrate run --database-url ${dbUrl}`);
    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    if (dbUrl.startsWith(localDbUrl) && !isLocalSetup && !skipDbPreparation) {
        // Dont't do this preparation for local (docker) setup - as it requires full cargo compilation.
        await utils.spawn(
            `cargo sqlx prepare --check --database-url ${dbUrl} -- --tests || cargo sqlx prepare --database-url ${dbUrl} -- --tests`
        );
    }

    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function setup(opts: DbOpts, skipDbPreparation: boolean) {
    let dals = getDals(opts);
    for (const [dalPath, dbUrl] of dals.entries()) {
        await setupForDal(dalPath, dbUrl, skipDbPreparation);
    }
}

async function waitForDal(dbUrl: string, tries: number) {
    for (let i = 0; i < tries; i++) {
        const result = await utils.allowFail(utils.exec(`pg_isready -d "${dbUrl}"`));
        if (result !== null) return; // null means failure
        console.log(`waiting for postgres ${dbUrl}`);
        await utils.sleep(1);
    }
    await utils.exec(`pg_isready -d "${dbUrl}"`);
}

export async function wait(opts: DbOpts, tries: number = 4) {
    let dals = getDals(opts);
    for (const dbUrl of dals.values()) {
        await waitForDal(dbUrl, tries);
    }
}

async function prepareSqlxDataForDal(dalPath: DalPath, dbUrl: string, check: boolean) {
    process.chdir(dalPath);
    let check_string = '';
    if (check) {
        check_string = '--check';
    }
    await utils.spawn(`cargo sqlx prepare ${check_string} --database-url ${dbUrl} -- --tests`);
    process.chdir(process.env.ZKSYNC_HOME as string);
}

export async function prepareSqlxData(opts: DbOpts, check: boolean) {
    let dals = getDals(opts);
    for (const [dalPath, dbUrl] of dals.entries()) {
        await prepareSqlxDataForDal(dalPath, dbUrl, check);
    }
}

interface DbGenerateMigrationOpts {
    prover: string | undefined;
    core: string | undefined;
}

export const command = new Command('db').description('database management');

command.command('drop').description('drop the database').option('-p, --prover').option('-c, --core').action(drop);
command.command('migrate').description('run migrations').option('-p, --prover').option('-c, --core').action(migrate);
command
    .command('new-migration')
    .description('generate a new migration for a specific database')
    .option('-p, --prover <name>')
    .option('-c, --core <name>')
    .action((opts: DbGenerateMigrationOpts) => {
        if ((!opts.prover && !opts.core) || (opts.prover && opts.core)) {
            throw new Error(
                '[aborted] please specify a single database to generate migration for (i.e. to generate a migration for server `zk db new-migration --core name_of_migration`'
            );
        }
        if (opts.prover) {
            return generateMigration(DbType.Prover, opts.prover);
        }
        return generateMigration(DbType.Core, opts.core!);
    });
command
    .command('setup')
    .description('initialize the database and perform migrations')
    .option('-p, --prover')
    .option('-c, --core')
    .action(setup);
command
    .command('wait')
    .description('wait for database to get ready for interaction')
    .option('-p, --prover')
    .option('-c, --core')
    .action(wait);
command
    .command('reset')
    .description('reinitialize the database')
    .option('-p, --prover')
    .option('-c, --core')
    .action(reset);
command
    .command('reset-test')
    .description('reinitialize the database for test')
    .option('-p, --prover')
    .option('-c, --core')
    .action(resetTest);
command
    .command('check-sqlx-data')
    .description('check sqlx-data.json is up to date')
    .option('-p, --prover')
    .option('-c, --core')
    .action(async (cmd) => {
        await prepareSqlxData(cmd, true);
    });
command
    .command('prepare')
    .description('check sqlx-data.json is up to date')
    .option('-p, --prover')
    .option('-c, --core')
    .action(async (cmd) => {
        await prepareSqlxData(cmd, false);
    });

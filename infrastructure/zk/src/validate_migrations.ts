import * as utils from './utils';
import fs from 'fs';

async function validateMigration(filePath: string) {
    const content = await fs.promises.readFile(filePath, { encoding: 'utf-8' });
    const isBackwardsCompatible = !content.includes('RENAME') && !content.includes('DROP');
    if (!isBackwardsCompatible) {
        console.warn(`Found not backwards-compatible migration in ${filePath}!`);
    }
    return isBackwardsCompatible;
}

async function getWhitelistedMigrations() {
    const whitelistFilepath = 'not-backwards-compatible-migrations-whitelist.txt';
    const whitelistedFilesRaw = await fs.promises.readFile(whitelistFilepath, { encoding: 'utf-8' });
    //filtering out comments and empty lines
    return whitelistedFilesRaw
        .split('\n')
        .map((x) => x.trim())
        .filter((x) => !x.startsWith('//'))
        .filter((x) => x.length);
}

export async function validateMigrations() {
    const whitelistedMigrations = await getWhitelistedMigrations();
    let { stdout: filesRaw } = await utils.exec('find core/lib/dal -type f -name "*up.sql"');
    const files = filesRaw
        .trim()
        .split('\n')
        .sort()
        .filter((filepath) => !whitelistedMigrations.includes(filepath));
    const validateResults = await Promise.all(files.map((file) => validateMigration(file)));
    if (validateResults.includes(false)) {
        throw new Error(
            'A new migration that is not backwards-compatible was detected! As such migrations can easily cause an outage,' +
                'they have to be explicitly added to a whitelist: not-backwards-compatible-migrations-whitelist.txt'
        );
    }
}

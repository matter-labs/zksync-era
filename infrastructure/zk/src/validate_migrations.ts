import * as utils from './utils';
import fs from 'fs';

async function validateMigration(filePath: string) {
    let content = await fs.promises.readFile(filePath, { encoding: 'utf-8' });
    content = content.toUpperCase();
    const isBackwardsCompatible = !content.includes('RENAME') && !content.includes('DROP');
    if (!isBackwardsCompatible) {
        console.warn(`Found not backwards-compatible migration in ${filePath}!`);
    }
    return isBackwardsCompatible;
}

async function getallowlistedMigrations() {
    const allowlistFilepath = 'not-backwards-compatible-migrations-allowlist.txt';
    const allowlistedFilesRaw = await fs.promises.readFile(allowlistFilepath, { encoding: 'utf-8' });
    //filtering out comments and empty lines
    return allowlistedFilesRaw
        .split('\n')
        .map((x) => x.trim())
        .filter((x) => !x.startsWith('//'))
        .filter((x) => x.length);
}

export async function validateMigrations() {
    const allowlistedMigrations = await getallowlistedMigrations();
    let { stdout: filesRaw } = await utils.exec('find core/lib/dal -type f -name "*up.sql"');
    const files = filesRaw
        .trim()
        .split('\n')
        .sort()
        .filter((filepath) => !allowlistedMigrations.includes(filepath));
    const validateResults = await Promise.all(files.map((file) => validateMigration(file)));
    if (validateResults.includes(false)) {
        throw new Error(
            'A new migration that is not backwards-compatible was detected! As such migrations can easily cause an outage,' +
                'they have to be explicitly added to an allowlist: not-backwards-compatible-migrations-allowlist.txt'
        );
    }
}

import { join, dirname, resolve } from 'path';
import { existsSync } from 'fs';

/**
 * Finds the zksync home directory by looking for the "core" directory
 * in the current path or any parent directories.
 *
 * @returns The absolute path to the zksync home directory (parent of "core")
 * @throws Error if "core" directory cannot be found
 */
export function findHome(): string {
    let currentPath = process.cwd();

    // Walk up the directory tree looking for "core" directory
    while (currentPath !== dirname(currentPath)) {
        const corePath = join(currentPath, 'core');

        if (existsSync(corePath)) {
            // Found the "core" directory, return its parent
            return resolve(currentPath);
        }

        // Move up to parent directory
        currentPath = dirname(currentPath);
    }

    throw new Error('Could not find "core" directory in current path or any parent directories');
}

/**
 * Gets the absolute path to the logs directory
 *
 * @returns The absolute path to the logs directory
 */
export function logsPath(): string {
    return join(findHome(), 'logs');
}

/**
 * Gets the absolute path to the chains directory
 *
 * @returns The absolute path to the chains directory
 */
export function chainsPath(): string {
    return join(findHome(), 'chains');
}

/**
 * Gets the absolute path to the configs directory
 *
 * @returns The absolute path to the configs directory
 */
export function configsPath(): string {
    return join(findHome(), 'configs');
}

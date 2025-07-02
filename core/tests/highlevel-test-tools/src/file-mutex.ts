import * as fs from 'fs';
import * as path from 'path';

/**
 * Simple file-based mutex implementation for Node.js
 */
export class FileMutex {
    private lockFile: string;
    private lockDir: string;

    constructor() {
        this.lockDir = '.';
        this.lockFile = path.join(this.lockDir, 'highlevel_tests.lock');
    }

    async acquire(): Promise<void> {
        const maxRetries = 600 * 10; // 10 minutes with 100ms intervals
        const retryDelay = 100; // 100ms

        for (let i = 0; i < maxRetries; i++) {
            try {
                // Try to create the lock file exclusively
                const fd = fs.openSync(this.lockFile, 'wx');
                fs.closeSync(fd);

                // Write current process info to lock file
                const processInfo = {
                    pid: process.pid,
                    timestamp: new Date().toISOString(),
                    command: process.argv.join(' ')
                };
                fs.writeFileSync(this.lockFile, JSON.stringify(processInfo, null, 2));

                console.log(`🔒 Acquired mutex lock: ${this.lockFile}`);
                return;
            } catch (error: any) {
                if (error.code === 'EEXIST') {
                    await new Promise((resolve) => setTimeout(resolve, retryDelay));
                } else {
                    throw error;
                }
            }
        }

        throw new Error(`Failed to acquire mutex lock after ${maxRetries} retries: ${this.lockFile}`);
    }

    release(): void {
        try {
            fs.unlinkSync(this.lockFile);
            console.log(`🔓 Released mutex lock: ${this.lockFile}`);
        } catch (error) {
            console.warn(`Warning: Failed to release mutex lock: ${this.lockFile}`, error);
        }
    }
}

/**
 * Removes all test chain configurations from the chains directory
 */
export function cleanTestChains(chainsDir: string = './chains'): void {
    if (!fs.existsSync(chainsDir)) {
        console.log(`📁 Chains directory ${chainsDir} does not exist, nothing to clean`);
        return;
    }

    const files = fs.readdirSync(chainsDir);
    let removedCount = 0;

    for (const file of files) {
        const filePath = path.join(chainsDir, file);
        try {
            const stat = fs.statSync(filePath);

            // Check if it's a directory and matches test chain pattern (contains underscore followed by 8 hex chars)
            if (stat.isDirectory() && /^[a-z_]+_[a-f0-9]{8}$/.test(file)) {
                fs.rmSync(filePath, { recursive: true, force: true });
                removedCount++;
                console.log(`🗑️  Removed test chain: ${file}`);
            }
        } catch (error: any) {
            if (error.code !== 'ENOENT') {
                console.warn(`⚠️  Failed to remove test chain ${file}:`, error.message);
            }
        }
    }

    console.log(`🧹 Cleaned ${removedCount} test chain configurations from ${chainsDir}`);
}

/**
 * Cleans up any leftover mutex lock files from previous test runs
 */
export function cleanMutexLockFiles(): void {
    const mutexLockFile = 'highlevel_tests.lock';
    if (fs.existsSync(mutexLockFile)) {
        try {
            fs.unlinkSync(mutexLockFile);
            console.log('🧹 Cleaned up leftover mutex lock file from previous test run');
        } catch (error) {
            console.warn('⚠️  Warning: Could not remove leftover mutex lock file:', error);
        }
    }
}

import * as fs from 'fs';
import * as path from 'path';
import { findHome } from './zksync-home';

/**
 * Maximum age (in ms) before a lock is considered stale regardless of PID liveness.
 * Guards against PID reuse: even if the PID is alive, a very old lock was almost
 * certainly left by a dead process whose PID was recycled.
 *
 * Must be longer than the longest critical section. The heaviest holder is
 * create-chain.ts which runs `chain create` + `chain init` under the lock
 * and can take 5–10 minutes under parallel load. 15 minutes gives safe headroom.
 */
const STALE_LOCK_AGE_MS = 15 * 60 * 1000;

/**
 * Simple file-based mutex implementation for Node.js
 */
export class FileMutex {
    private lockFile: string;
    private lockDir: string;

    constructor() {
        try {
            this.lockDir = findHome();
        } catch (_) {
            this.lockDir = '.';
        }
        this.lockFile = path.join(this.lockDir, 'highlevel_tests.lock');
    }

    async acquire(): Promise<void> {
        const maxRetries = 600 * 12; // 12 minutes with 100ms intervals
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
                    // Check if the existing lock is stale (dead owner or too old)
                    if (this.tryRecoverStaleLock()) {
                        // Lock was stale and removed — retry immediately
                        continue;
                    }
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

    /**
     * Checks whether the current lock holder is still alive. If the owning
     * process is dead or the lock is older than STALE_LOCK_AGE_MS, removes
     * the lock file and returns true so the caller can retry acquisition.
     */
    private tryRecoverStaleLock(): boolean {
        try {
            const content = fs.readFileSync(this.lockFile, 'utf8');
            const info = JSON.parse(content) as { pid?: number; timestamp?: string; command?: string };

            const lockAge = info.timestamp ? Date.now() - new Date(info.timestamp).getTime() : Infinity;

            // If the lock is older than the threshold, treat as stale regardless of PID.
            // This guards against PID reuse on long-running CI hosts.
            if (lockAge > STALE_LOCK_AGE_MS) {
                console.warn(
                    `Removing stale mutex lock (age: ${(lockAge / 1000).toFixed(0)}s, ` +
                        `pid: ${info.pid}, command: ${info.command}): ${this.lockFile}`
                );
                fs.unlinkSync(this.lockFile);
                return true;
            }

            // If PID is present, check whether the process is still alive.
            if (info.pid) {
                try {
                    process.kill(info.pid, 0); // signal 0 = existence check, no actual signal sent
                } catch {
                    // process.kill throws if the PID doesn't exist — lock is stale.
                    console.warn(
                        `Removing stale mutex lock (owner pid ${info.pid} is dead, ` +
                            `age: ${(lockAge / 1000).toFixed(0)}s): ${this.lockFile}`
                    );
                    fs.unlinkSync(this.lockFile);
                    return true;
                }
            }
        } catch {
            // If we can't read/parse the lock file, leave it alone and let the
            // normal retry loop handle it.
        }
        return false;
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
    let mutexLockFile = 'highlevel_tests.lock';
    try {
        mutexLockFile = path.join(findHome(), 'highlevel_tests.lock');
    } catch (_) {}
    if (fs.existsSync(mutexLockFile)) {
        try {
            fs.unlinkSync(mutexLockFile);
            console.log('🧹 Cleaned up leftover mutex lock file from previous test run');
        } catch (error) {
            console.warn('⚠️  Warning: Could not remove leftover mutex lock file:', error);
        }
    }
}

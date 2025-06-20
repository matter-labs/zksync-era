import { exec as _exec, spawn as _spawn } from 'child_process';
import { promisify } from 'util';
export * from './node-spawner';

export type { ChildProcess } from 'child_process';

/** Logs information with additional data (e.g., a timestamp). */
export function log(message: string, ...args: any[]) {
    console.log(`[${new Date().toISOString()}] ${message}`, ...args);
}

// async executor of shell commands
// spawns a new shell and can execute arbitrary commands, like "ls -la | grep .env"
// returns { stdout, stderr }
const promisified = promisify(_exec);

export function exec(command: string) {
    command = command.replace(/\n/g, ' ');
    return promisified(command);
}

// executes a command in a new shell
// but pipes data to parent's stdout/stderr
export function spawn(command: string) {
    command = command.replace(/\n/g, ' ');
    log(`+ ${command}`);
    const child = _spawn(command, { stdio: 'inherit', shell: true });
    return new Promise((resolve, reject) => {
        child.on('error', reject);
        child.on('close', (code) => {
            code == 0 ? resolve(code) : reject(`Child process exited with code ${code}`);
        });
    });
}

export async function sleep(seconds: number) {
    return new Promise((resolve) => setTimeout(resolve, seconds * 1000));
}

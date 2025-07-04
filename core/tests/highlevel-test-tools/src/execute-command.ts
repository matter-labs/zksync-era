import { ChildProcess, spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { getLogsDirectory, markLogsDirectoryAsFailed } from './logs';
import * as console from 'node:console';
import type { ProcessEnvOptions } from 'node:child_process';

/**
 * Strips ANSI escape sequences from a string
 * @param str - The string to clean
 * @returns The string with ANSI escape sequences removed
 */
function stripAnsiEscapeCodes(str: string): string {
    // Remove ANSI escape sequences (colors, cursor movements, etc.)
    return str.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '');
}

/**
 * Reads the last N lines from a file
 * @param filePath - The path to the file
 * @param numLines - The number of lines to read from the end
 * @returns The last N lines as a string, or empty string if file doesn't exist
 */
function readLastNLines(filePath: string, numLines: number): string {
    try {
        if (!fs.existsSync(filePath)) {
            return '';
        }

        const content = fs.readFileSync(filePath, 'utf8');
        const lines = content.split('\n');

        // Get the last N lines, but don't include empty lines at the end
        const lastLines = lines.slice(-numLines).filter((line) => line.trim() !== '');

        return lastLines.join('\n');
    } catch (error) {
        return `Error reading log file: ${error}`;
    }
}

/**
 * Logs a command to the executed commands log file
 * @param chainName - The chain name
 * @param command - The command being executed
 * @param args - The command arguments
 * @param startTime - The start time of the command
 * @param endTime - The end time of the command (optional for detached commands)
 * @param isDetached - Whether this is a detached/background command
 * @param failed - Whether the command failed
 */
function logExecutedCommand(
    chainName: string,
    command: string,
    args: string[],
    startTime: number,
    endTime?: number,
    isDetached: boolean = false,
    failed: boolean = false
): void {
    const logsDir = getLogsDirectory(chainName);

    // Create executed commands log file for this chain
    const executedCommandsLogFile = path.join(logsDir, 'executed_commands.log');

    const timestamp = new Date(startTime).toISOString();
    const fullCommand = `${command} ${args.join(' ')}`;

    let timeInfo: string;
    if (isDetached) {
        timeInfo = '(detached)';
    } else if (endTime) {
        const duration = endTime - startTime;
        const failedPrefix = failed ? '[❌FAIL] ' : '';
        timeInfo = `(${duration.toString().padStart(6)}ms)${failedPrefix}`;
    } else {
        timeInfo = '(running)';
    }

    // Log command with timestamp and time info, padded so commands align
    const logEntry = `[${timestamp}] ${timeInfo} ${fullCommand}\n`;
    fs.appendFileSync(executedCommandsLogFile, logEntry);
}

function findAndPrintErrorFromLogs(logFilePath: string) {
    // Print last 60 lines of logs on error
    const lastLogs = readLastNLines(logFilePath, 60);
    if (lastLogs) {
        console.error('\n📋 Last 60 lines of logs:');
        console.error('');
        console.error('─'.repeat(120));
        console.error(lastLogs);
        console.error('─'.repeat(120));
        console.error('');
    }
}

/**
 * Executes a command and returns a promise
 */
export async function executeCommand(
    command: string,
    args: string[],
    chainName: string,
    logFileName: string,
    runInBackground: boolean = false,
    extraOptions: ProcessEnvOptions = {}
): Promise<ChildProcess> {
    const logsDir = getLogsDirectory(chainName);
    const startTime = Date.now();

    return new Promise((resolve, reject) => {
        console.log(`Executing: ${command} ${args.join(' ')}`);

        // Ensure logs directory exists
        if (!fs.existsSync(logsDir)) {
            fs.mkdirSync(logsDir, { recursive: true });
        }

        // Create log file for this chain (one file per chain, no timestamp)
        const logFileNameWithExtension = `${logFileName}.log`;
        const logFilePath = path.join(logsDir, logFileNameWithExtension);

        // Create write stream for the log file (append mode)
        const logStream = fs.createWriteStream(logFilePath, { flags: 'a' });

        // Write command to log
        const logEntry = `[${new Date().toISOString()}] Executing: ${command} ${args.join(' ')}\n`;
        logStream.write(logEntry);

        const child = spawn(command, args, {
            stdio: ['inherit', 'pipe', 'pipe'],
            shell: true,
            detached: runInBackground,
            ...extraOptions
        });

        // Pipe stdout and stderr only to log file (not to console) with ANSI escape codes stripped
        child.stdout?.on('data', (data) => {
            const output = stripAnsiEscapeCodes(data.toString());
            logStream.write(output);
        });

        child.stderr?.on('data', (data) => {
            const output = stripAnsiEscapeCodes(data.toString());
            logStream.write(output);
        });

        child.on('close', (code, signal) => {
            const endTime = Date.now();

            // Log the command completion to executed commands log
            const failed = code !== 0 && code !== 137 && signal !== 'SIGKILL';
            logExecutedCommand(chainName, command, args, startTime, endTime, false, failed);

            const closeMessage = `[${new Date().toISOString()}] Command finished with exit code: ${code} and signal: ${signal}\n`;
            logStream.write(closeMessage);
            logStream.end();

            if (code === 0) {
                console.log(`✅ Command completed successfully. Logs saved to: ${logFilePath}`);
                resolve(child);
            } else {
                const errorMessage = `Command ${command} ${args.join(' ')} failed with exit code ${code}. Check logs at: ${logFilePath}`;
                console.error(`❌ ${errorMessage}`);

                findAndPrintErrorFromLogs(logFilePath);

                // Rename the log directory to indicate failure
                markLogsDirectoryAsFailed(chainName);

                reject(new Error(errorMessage));
            }
        });

        child.on('error', (error) => {
            console.log(`Command error: ${JSON.stringify(error)}, ${error.message}, ${typeof error}`);
            const endTime = Date.now();

            // Log the command error to executed commands log
            logExecutedCommand(chainName, command, args, startTime, endTime, false, true);

            const errorMessage = `[${new Date().toISOString()}] Command error: ${error.message}\n`;
            logStream.write(errorMessage);
            logStream.end();

            findAndPrintErrorFromLogs(logFilePath);

            // Rename the log directory to indicate failure
            markLogsDirectoryAsFailed(chainName);

            reject(error);
        });

        if (runInBackground) {
            resolve(child);
        }
    });
}

export async function executeBackgroundCommand(
    command: string,
    args: string[],
    chainName: string,
    logFileName: string
): Promise<ChildProcess> {
    return executeCommand(command, args, chainName, logFileName, true);
}

export function removeErrorListeners(process: ChildProcess) {
    process.removeAllListeners('close');
    process.removeAllListeners('error');
}

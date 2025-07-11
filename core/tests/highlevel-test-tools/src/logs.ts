import * as fs from 'node:fs';
import { join } from 'path';
import { logsPath } from './zksync-home';

export let chainNameToTestSuite: Map<string, string> = new Map();

export function getLogsDirectory(chainName: string) {
    const suiteName = chainNameToTestSuite.get(chainName) ?? '';

    const logsDir = join(logsPath(), 'highlevel', `[${suiteName}] ${chainName}`);
    if (!fs.existsSync(logsDir)) {
        fs.mkdirSync(logsDir, { recursive: true });
    }
    const failedLogsDir = join(logsPath(), 'highlevel', `[❌FAIL] [${suiteName}] ${chainName}`);
    if (fs.existsSync(failedLogsDir)) {
        return failedLogsDir;
    }

    return logsDir;
}

export function markLogsDirectoryAsFailed(chainName: string) {
    const suiteName = chainNameToTestSuite.get(chainName) ?? '';

    // Rename the log directory to indicate failure
    const failedLogsDir = join(logsPath(), 'highlevel', `[❌FAIL] [${suiteName}] ${chainName}`);
    try {
        if (fs.existsSync(getLogsDirectory(chainName))) {
            fs.renameSync(getLogsDirectory(chainName), failedLogsDir);
            console.log(`📁 Renamed log directory to: ${failedLogsDir}`);
        }
    } catch (renameError) {
        console.warn(`⚠️ Failed to rename log directory: ${renameError}`);
    }
}

/**
 * Removes all historical log files from the specified directory
 */
export function cleanHistoricalLogs(): void {
    const logsDir = join(logsPath(), 'highlevel');
    if (fs.existsSync(logsDir)) {
        fs.rmSync(logsDir, { recursive: true, force: true });
    }
}

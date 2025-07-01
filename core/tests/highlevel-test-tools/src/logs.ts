import * as fs from "node:fs";

export function getLogsDirectory(chainName: string) {

    // vitest returns it in format `{filePath} > ${describeTitle} > ${testTitle}`
    const suiteName = expect.getState().currentTestName?.split('>')[1].trim() ?? '';

    const logsDir = `../../../logs/highlevel/[${suiteName}] ${chainName}`;
    if (!fs.existsSync(logsDir)) {
        fs.mkdirSync(logsDir, {recursive: true});
    }
    const failedLogsDir = `../../../logs/highlevel/[❌FAIL] [${suiteName}] ${chainName}`;
    if (fs.existsSync(failedLogsDir)) {
        return failedLogsDir;
    }

    return logsDir;
}

export function markLogsDirectoryAsFailed(chainName: string) {
    // vitest returns it in format `{filePath} > ${describeTitle} > ${testTitle}`
    const suiteName = expect.getState().currentTestName?.split('>')[1] ?? '';

    // Rename the log directory to indicate failure
    const failedLogsDir = `../../../logs/highlevel/[❌FAIL] [${suiteName}] ${chainName}`;
    try {
        if (fs.existsSync(getLogsDirectory(chainName))) {
            fs.renameSync(getLogsDirectory(chainName), failedLogsDir);
            console.log(`📁 Renamed log directory to: ${failedLogsDir}`);
        }
    }
    catch (renameError) {
        console.warn(`⚠️ Failed to rename log directory: ${renameError}`);
    }
}

/**
 * Removes all historical log files from the specified directory
 */
export function cleanHistoricalLogs(): void {
    const logsDir = '../../../logs/highlevel'
    if (fs.existsSync(logsDir)) {
        fs.rmSync(logsDir, {recursive: true, force: true});
    }
}

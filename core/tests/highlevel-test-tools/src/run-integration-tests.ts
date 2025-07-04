import { executeCommand } from './execute-command';
import * as fs from 'fs';
import { FileMutex } from './file-mutex';
import { getLogsDirectory } from './logs';

const fileMutex = new FileMutex();

/**
 * Appends server logs from the specific server log file to the main chain log file
 * @param chainName - The chain name
 * @param testName - The test name for log file suffix
 */
function appendServerLogs(chainName: string, testName: string): void {
    const logsDir = getLogsDirectory(chainName);
    const serverLogPath = `${logsDir}/server.log`;
    const mainLogPath = `${logsDir}/${testName}_tests.log`;

    if (fs.existsSync(serverLogPath) && fs.existsSync(mainLogPath)) {
        try {
            const serverLogContent = fs.readFileSync(serverLogPath, 'utf8');
            if (serverLogContent.trim()) {
                const separator = `\n[${new Date().toISOString()}] === Server logs from ${serverLogPath} ===\n`;
                fs.appendFileSync(mainLogPath, separator);
                fs.appendFileSync(mainLogPath, serverLogContent);
                fs.appendFileSync(mainLogPath, '\n');
                console.log(`📋 Appended server logs to ${mainLogPath}`);
            }
        } catch (error) {
            console.warn(`⚠️ Could not append server logs from ${serverLogPath}: ${error}`);
        }
    }
}

export async function initTestWallet(chainName: string): Promise<void> {
    console.log(`🔑 Initializing test wallet for chain: ${chainName}`);
    await fileMutex.acquire();
    try {
        await executeCommand(
            'zkstack',
            ['dev', 'init-test-wallet', '--chain', chainName],
            chainName,
            'init_test_wallet'
        );
    } finally {
        fileMutex.release();
    }
}

async function runTest(
    testName: string,
    chainName: string,
    testPattern: string | undefined = undefined,
    additionalArgs: string[] = []
): Promise<void> {
    const emojiMap: Record<string, string> = {
        integration: '🧪',
        fees: '💰',
        revert: '🔄',
        upgrade: '⬆️',
        recovery: '🛟'
    };
    const emoji = emojiMap[testName] || '🧪';
    console.log(
        `${emoji} Running ${testName} tests for chain: ${chainName}${testPattern ? ` with pattern: ${testPattern}` : ''}`
    );
    const command = 'zkstack';
    const args = ['dev', 'test', testName, '--no-deps', '--chain', chainName, ...additionalArgs];
    if (testPattern) {
        args.push(`--test-pattern='${testPattern}'`);
    }
    try {
        const en_prefix = additionalArgs.includes('--external-node') ? 'en_' : '';
        await executeCommand(command, args, chainName, `${en_prefix}${testName}_tests`);
        appendServerLogs(chainName, testName);

        console.log(`✅ ${testName} tests completed successfully for chain: ${chainName}`);
    } catch (error) {
        console.error(`❌ ${testName} tests failed for chain: ${chainName}`, error);
        throw error;
    }
}

export async function runIntegrationTests(chainName: string, testPattern?: string): Promise<void> {
    await initTestWallet(chainName);
    await runTest('integration', chainName, testPattern, ['--verbose', '--ignore-prerequisites']);
}

export async function feesTest(chainName: string): Promise<void> {
    await initTestWallet(chainName);
    await runTest('fees', chainName, undefined, ['--no-kill']);
}

export async function revertTest(chainName: string): Promise<void> {
    await initTestWallet(chainName);
    await runTest('revert', chainName, undefined, ['--no-kill', '--ignore-prerequisites']);
}

export async function upgradeTest(chainName: string): Promise<void> {
    await initTestWallet(chainName);
    await runTest('upgrade', chainName, undefined, []);
}

export async function snapshotsRecoveryTest(chainName: string): Promise<void> {
    await initTestWallet(chainName);
    await runTest('recovery', chainName, undefined, ['--snapshot', '--ignore-prerequisites', '--verbose']);
}

export async function genesisRecoveryTest(chainName: string): Promise<void> {
    await initTestWallet(chainName);
    await runTest('recovery', chainName, undefined, ['--no-kill', '--ignore-prerequisites', '--verbose']);
}

export async function enIntegrationTests(chainName: string): Promise<void> {
    await initTestWallet(chainName);
    await runTest('integration', chainName, undefined, ['--verbose', '--ignore-prerequisites', '--external-node']);
}

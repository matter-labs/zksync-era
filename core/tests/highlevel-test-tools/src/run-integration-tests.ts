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
    additionalArgs: string[] = [],
    options: { env?: NodeJS.ProcessEnv; logName?: string } = {}
): Promise<void> {
    const emojiMap: Record<string, string> = {
        integration: '🧪',
        fees: '💰',
        revert: '🔄',
        upgrade: '⬆️',
        recovery: '🛟'
    };
    const emoji = emojiMap[testName] || '🧪';
    // Names the run in console output and log files; differs from `testName` when one zkstack
    // test command is invoked more than once per chain (e.g. `integration` and then `force_fail`).
    const label = options.logName ?? testName;
    console.log(
        `${emoji} Running ${label} tests for chain: ${chainName}${testPattern ? ` with pattern: ${testPattern}` : ''}`
    );
    const command = 'zkstack';
    const args = ['dev', 'test', testName, '--no-deps', '--chain', chainName, ...additionalArgs];
    if (testPattern) {
        args.push(`--test-pattern='${testPattern}'`);
    }
    try {
        const en_prefix = additionalArgs.includes('--external-node') ? 'en_' : '';
        const logName = `${en_prefix}${label}`;
        await executeCommand(command, args, chainName, `${logName}_tests`, false, {
            env: options.env ? { ...process.env, ...options.env } : undefined
        });
        appendServerLogs(chainName, logName);

        console.log(`✅ ${label} tests completed successfully for chain: ${chainName}`);
    } catch (error) {
        console.error(`❌ ${label} tests failed for chain: ${chainName}`, error);
        throw error;
    }
}

export async function runIntegrationTests(
    chainName: string,
    secondChainName?: string | undefined,
    testPattern?: string
): Promise<void> {
    await initTestWallet(chainName);
    await runTest('integration', chainName, testPattern, [
        '--verbose',
        '--ignore-prerequisites',
        secondChainName ? `--second-chain=${secondChainName}` : ''
    ]);
    // The force-fail suite predicts the next priority op id, so it cannot share the chain with
    // suites that enqueue priority ops. It runs once they are done; jest skips it otherwise.
    if (!testPattern) {
        await runTest('integration', chainName, undefined, ['--verbose', '--ignore-prerequisites', 'force-fail'], {
            env: { RUN_FORCE_FAIL_TEST: '1' },
            logName: 'force_fail'
        });
    }
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

export async function enIntegrationTests(chainName: string, secondChainName?: string | undefined): Promise<void> {
    await initTestWallet(chainName);
    await runTest('integration', chainName, undefined, [
        '--verbose',
        '--ignore-prerequisites',
        '--external-node',
        secondChainName ? `--second-chain=${secondChainName}` : ''
    ]);
}

export async function gatewayMigrationToGatewayTest(chainName: string): Promise<void> {
    await initTestWallet(chainName);
    await runTest('gateway-migration', chainName, undefined, ['--to-gateway']);
}

export async function gatewayMigrationFromGatewayTest(chainName: string): Promise<void> {
    await initTestWallet(chainName);
    await runTest('gateway-migration', chainName, undefined, ['--from-gateway']);
}

export async function tokenBalanceMigrationTest(chainName: string): Promise<void> {
    await initTestWallet(chainName);
    await runTest('token-balance-migration', chainName, undefined);
}

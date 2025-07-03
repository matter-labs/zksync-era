import { executeCommand } from './execute-command';
import { getStepTimer } from './timing-tools';
import * as path from 'path';
import * as fs from 'fs';

/**
 * Appends server logs from the specific server log file to the main chain log file
 * @param chainName - The chain name
 * @param testName - The test name for log file suffix
 */
function appendServerLogs(chainName: string, testName: string): void {
  const serverLogPath = `../../../logs/server/${chainName}/server.log`;
  const mainLogPath = `../../../logs/highlevel/${chainName}_${testName}_tests.log`;
  
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

async function runTest(
  testName: string,
  chainName: string,
  testPattern: string | undefined = undefined,
  additionalArgs: string[] = []
): Promise<void> {
  const emojiMap: Record<string, string> = {
    'integration': '🧪',
    'fees': '💰',
    'revert': '🔄',
    'upgrade': '⬆️',
    'recovery': '🛟'
  };
  const emoji = emojiMap[testName] || '🧪';
  console.log(`${emoji} Running ${testName} tests for chain: ${chainName}${testPattern ? ` with pattern: ${testPattern}` : ''}`);
  const timer = getStepTimer(chainName);
  const command = 'zkstack';
  const args = ['dev', 'test', testName, '--no-deps', '--chain', chainName, ...additionalArgs];
  if (testPattern) {
    args.push(`--test-pattern='${testPattern}'`);
  }
  try {
    timer.startStep(`${testName} tests execution`);
    await executeCommand(command, args, chainName, `${testName}_tests`);
    timer.endStep(`${testName} tests execution`);
    
    // Append server logs after test completion
    appendServerLogs(chainName, testName);
    
    timer.logTotalTime();
    console.log(`✅ ${testName} tests completed successfully for chain: ${chainName}`);
  } catch (error) {
    console.error(`❌ ${testName} tests failed for chain: ${chainName}`, error);
    throw error;
  }
}

export async function runIntegrationTests(chainName: string, testPattern?: string): Promise<void> {
  const timer = getStepTimer(chainName);
  await runTest('integration', chainName, testPattern, ['--verbose', '--ignore-prerequisites']);
}

export async function feesTest(chainName: string): Promise<void> {
  await runTest('fees', chainName, undefined, ['--no-kill']);
}

export async function revertTest(chainName: string): Promise<void> {
  await runTest('revert', chainName, undefined, ['--no-kill', '--ignore-prerequisites']);
}

export async function upgradeTest(chainName: string): Promise<void> {
  await runTest('upgrade', chainName, undefined, []);
}

export async function snapshotsRecoveryTest(chainName: string): Promise<void> {
  await runTest('recovery', chainName, undefined, ['--snapshot', '--no-deps', '--ignore-prerequisites', '--verbose']);
}

export async function genesisRecoveryTest(chainName: string): Promise<void> {
  await runTest('recovery', chainName, undefined, ['--no-deps', '--no-kill', '--ignore-prerequisites', '--verbose']);
}

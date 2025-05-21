import { TestMaster } from '../src';
import fs from 'node:fs/promises';
import { ChildProcess, spawn } from 'node:child_process';
import fetch from 'node-fetch';
import { RetryableWallet } from '../src/retry-provider';
import * as zksync from 'zksync-ethers';
import { sleep } from 'utils';
import { shouldLoadConfigFromFile } from 'utils/build/file-configs';
import { injectPermissionsToFile } from '../src/private-rpc-permissions-editor';
import path from 'path';
import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';
import { Token } from '../src/types';
import * as ethers from 'ethers';
import { scaledGasPrice } from '../src/helpers';
import { logsTestPath } from 'utils/build/logs';
import {readFileSync} from "node:fs";
import YAML from 'yaml';

const chainName = shouldLoadConfigFromFile().chain;

describe('Tests for the private rpc', () => {
    let testMaster: TestMaster;
    let alice: RetryableWallet;
    let bob: zksync.Wallet;
    let tokenDetails: Token;
    let aliceErc20: zksync.Contract;

    let baseTokenAddress: string;

    function waitForProcess(childProcess: ChildProcess): Promise<void> {
        return new Promise((resolve, reject) => {
            childProcess.on('close', resolve);
            childProcess.on('exit', resolve);
            childProcess.on('disconnect', resolve);
            childProcess.on('error', reject);
        });
    }

    async function executeCommandWithLogs(command: string, logsPath: string) {
        const logs = await fs.open(logsPath, 'w');
        const childProcess = spawn(command, {
            cwd: process.env.ZKSYNC_HOME!!,
            stdio: ['ignore', logs.fd, logs.fd],
            shell: true
        });
        try {
            await waitForProcess(childProcess);
        } finally {
            sleep(5);
            childProcess.kill();
            await logs.close();
        }
    }

    async function logsPath(name: string): Promise<string> {
        return await logsTestPath(chainName, 'logs/prividium/', name);
    }

    async function waitForHealth(baseUrl: string, timeoutMs = 15_000, intervalMs = 250): Promise<void> {
        const deadline = Date.now() + timeoutMs;
        const url = `${baseUrl}/health`;

        while (Date.now() < deadline) {
            try {
                const res = await fetch(url);
                if (res.ok) return;
            } catch {}
            await new Promise((r) => setTimeout(r, intervalMs));
        }
        throw new Error(`waitForHealth: ${url} was not healthy after ${timeoutMs} ms`);
    }

    beforeAll(async () => {
        const initCommand = `zkstack private-rpc init --verbose --dev --chain ${chainName}`;
        const runCommand = `zkstack private-rpc run --verbose --chain ${chainName}`;

        await executeCommandWithLogs(initCommand, await logsPath('private-rpc-init.log'));
        executeCommandWithLogs(runCommand, await logsPath('private-rpc-run.log'));

        await waitForHealth(rpcUrl());
        testMaster = TestMaster.getInstance(__filename);

        tokenDetails = testMaster.environment().erc20Token;
        alice = await testMaster.privateRpcMainAccount(rpcUrl());
        bob = await testMaster.privateRpcNewEmptyAccount(rpcUrl());
        aliceErc20 = new zksync.Contract(tokenDetails.l2Address, zksync.utils.IERC20, alice);
        baseTokenAddress = await alice._providerL2().getBaseTokenContractAddress();
    });

    function rpcUrl() {
        const pathToHome = path.join(__dirname, '../../../..');
        const dockerPath = path.join(
            pathToHome,
            `chains/${chainName}/configs//private-proxy-docker-compose.yml`
        );
        const port = YAML.parse(readFileSync(dockerPath, 'utf8')).services['private-proxy'].environment.PORT
        return `http://localhost:${port}`;
    }

    function addPermission(contractAddress: string, methodSignature: string): Promise<void> {
        const pathToHome = path.join(__dirname, '../../../..');
        const permissionsPath = path.join(
            pathToHome,
            `chains/${chainName}/configs/private-rpc/private-rpc-permissions.yaml`
        );
        return injectPermissionsToFile(permissionsPath, contractAddress, methodSignature);
    }

    test('Creating access tokens works and tokens are unique', async () => {
        const testAddress = '0x4f9133d1d3f50011a6859807c837bdcb31aaab13';

        const token1 = await testMaster.privateRpcToken(rpcUrl(), testAddress);
        const token2 = await testMaster.privateRpcToken(rpcUrl(), testAddress);

        expect(typeof token1).toBe('string');
        expect(typeof token2).toBe('string');
        expect(token1).not.toBe(token2);
    });

    test('User can read his own balance and unable to read others', async () => {
        const testAddress = '0x4f9133d1d3f50011a6859807c837bdcb31aaab13';
        const otherTestAddress = '0xaaaaaaaaaaaaabbbb/bbbbbbbbbbbbbbccccccccc';
        const provider = await testMaster.privateRpcProvider(rpcUrl(), testAddress);
        const balance = await provider.getBalance(testAddress);

        expect(balance).toEqual(0n);

        await expect(provider.getBalance(otherTestAddress)).rejects.toThrow();
    });

    test('ERC-20 properties can be read', async () => {
        await addPermission(tokenDetails.l2Address, 'function name() public view returns (string)');
        await addPermission(tokenDetails.l2Address, 'function decimals() public view returns (uint8)');
        await addPermission(tokenDetails.l2Address, 'function symbol() public view returns (string)');

        await expect(aliceErc20.name()).resolves.toBe(tokenDetails.name);
        await expect(aliceErc20.decimals()).resolves.toBe(tokenDetails.decimals);
        await expect(aliceErc20.symbol()).resolves.toBe(tokenDetails.symbol);
    });

    test('ERC-20 transfers can be made', async () => {
        const value = 200n;

        await addPermission(
            tokenDetails.l2Address,
            'function transfer(address to, uint256 amount) public returns (bool)'
        );
        await addPermission(tokenDetails.l2Address, 'function balanceOf(address owner) public view returns (uint256)');

        const balanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: -value },
            { wallet: bob, change: value }
        ]);
        const feeCheck = await shouldOnlyTakeFee(alice);

        // Send transfer, it should succeed.
        await expect(aliceErc20.transfer(bob.address, value)).toBeAccepted([balanceChange, feeCheck]);
    });

    test('ERC-20 Incorrect transfer should revert', async () => {
        const noBalanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: 0n },
            { wallet: bob, change: 0n }
        ]);
        // Fee in ETH should be taken though.
        const feeTaken = await shouldOnlyTakeFee(alice);

        const absurdly_high_value = ethers.parseEther('1000000.0');
        const gasPrice = await scaledGasPrice(alice);
        const gasLimit = await aliceErc20.transfer.estimateGas(bob.address, 1);
        await expect(aliceErc20.transfer(bob.address, absurdly_high_value, { gasLimit, gasPrice })).toBeReverted([
            noBalanceChange,
            feeTaken
        ]);
    });

    test('Base token can be transferred', async () => {
        const value = 200n;

        // Transfer funds from Alice to Bob
        const bobAddress = await bob.getAddress();
        // special case for base token transfer
        await addPermission(bobAddress, '#BASE_TOKEN_TRANSFER');

        const balanceChange = await shouldChangeTokenBalances(baseTokenAddress, [{ wallet: bob, change: value }]);

        // Send transfer, it should succeed.
        await expect(await alice.transfer({ to: bobAddress, amount: value })).toBeAccepted([balanceChange]);
    });

    test('Forbidden RPC methods are blocked', async () => {
        const baseUrl = rpcUrl();
        const testAddress = '0x4f9133d1d3f50011a6859807c837bdcb31aaab13';
        const token = await testMaster.privateRpcToken(baseUrl, testAddress);

        const forbidden = [
            'debug_traceBlockByHash',
            'debug_traceBlockByNumber',
            'debug_traceCall',
            'debug_traceTransaction',
            'eth_accounts',
            'eth_getStorageAt',
            'eth_getTransactionByBlockHashAndIndex',
            'eth_newFilter',
            'eth_newPendingTransactionFilter',
            'zks_getProof'
        ];

        for (let i = 0; i < forbidden.length; i++) {
            const method = forbidden[i];
            const res = await fetch(`${baseUrl}/rpc/${token}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    jsonrpc: '2.0',
                    method,
                    params: [],
                    id: i + 1
                })
            });

            expect(res.ok).toBe(true);
            const body = await res.json();
            expect(body).toHaveProperty('error');
        }
    });
});

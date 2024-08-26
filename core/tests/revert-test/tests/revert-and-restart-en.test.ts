// Test of the behaviour of the external node when L1 batches get reverted.
//
// NOTE:
// main_contract.getTotalBatchesCommitted actually checks the number of batches committed.
// main_contract.getTotalBatchesExecuted actually checks the number of batches executed.
import * as utils from 'utils';
import { Tester } from './tester';
import { exec, runServerInBackground, runExternalNodeInBackground } from './utils';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect, assert } from 'chai';
import fs from 'fs';
import * as child_process from 'child_process';
import * as dotenv from 'dotenv';
import {
    getAllConfigsPath,
    loadConfig,
    shouldLoadConfigFromFile,
    replaceAggregatedBlockExecuteDeadline
} from 'utils/build/file-configs';
import path from 'path';
import { ChildProcessWithoutNullStreams } from 'child_process';
import { promisify } from 'node:util';

const pathToHome = path.join(__dirname, '../../../..');
const fileConfig = shouldLoadConfigFromFile();

let mainEnv: string;
let extEnv: string;

let deploymentMode: string;

if (fileConfig.loadFromFile) {
    const genesisConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'genesis.yaml' });
    deploymentMode = genesisConfig.deploymentMode;
} else {
    if (!process.env.DEPLOYMENT_MODE) {
        throw new Error('DEPLOYMENT_MODE is not set');
    }
    if (!['Validium', 'Rollup'].includes(process.env.DEPLOYMENT_MODE)) {
        throw new Error(`Unknown deployment mode: ${process.env.DEPLOYMENT_MODE}`);
    }
    deploymentMode = process.env.DEPLOYMENT_MODE;
}

if (deploymentMode == 'Validium') {
    mainEnv = process.env.IN_DOCKER ? 'dev_validium_docker' : 'dev_validium';
    extEnv = process.env.IN_DOCKER ? 'ext-node-validium-docker' : 'ext-node-validium';
} else {
    // Rollup deployment mode
    mainEnv = process.env.IN_DOCKER ? 'docker' : 'dev';
    extEnv = process.env.IN_DOCKER ? 'ext-node-docker' : 'ext-node';
}
const mainLogsPath: string = 'revert_main.log';
const extLogsPath: string = 'revert_ext.log';

interface SuggestedValues {
    lastExecutedL1BatchNumber: bigint;
    nonce: number;
    priorityFee: number;
}

// Parses output of "print-suggested-values" command of the revert block tool.
function parseSuggestedValues(jsonString: string): SuggestedValues {
    const json = JSON.parse(jsonString);
    assert(json && typeof json === 'object');
    assert(Number.isInteger(json.last_executed_l1_batch_number));
    assert(Number.isInteger(json.nonce));
    assert(Number.isInteger(json.priority_fee));
    return {
        lastExecutedL1BatchNumber: BigInt(json.last_executed_l1_batch_number),
        nonce: json.nonce,
        priorityFee: json.priority_fee
    };
}

function run(cmd: string, args: string[], options: child_process.SpawnOptions): child_process.SpawnSyncReturns<Buffer> {
    let res = child_process.spawnSync(cmd, args, options);
    expect(res.error).to.be.undefined;
    return res;
}

function compileBinaries() {
    console.log('compiling binaries');
    run(
        'cargo',
        ['build', '--release', '--bin', 'zksync_external_node', '--bin', 'zksync_server', '--bin', 'block_reverter'],
        { cwd: process.env.ZKSYNC_HOME }
    );
}

// Fetches env vars for the given environment (like 'dev', 'ext-node').
// TODO: it would be better to import zk tool code directly.
function fetchEnv(zksyncEnv: string): any {
    let res = run('./bin/zk', ['f', 'env'], {
        cwd: process.env.ZKSYNC_HOME,
        env: {
            PATH: process.env.PATH,
            ZKSYNC_ENV: zksyncEnv,
            ZKSYNC_HOME: process.env.ZKSYNC_HOME
        }
    });
    return { ...process.env, ...dotenv.parse(res.stdout) };
}

async function runBlockReverter(args: string[]): Promise<string> {
    let env = fetchEnv(mainEnv);

    let fileConfigFlags = '';
    if (fileConfig.loadFromFile) {
        const configPaths = getAllConfigsPath({ pathToHome, chain: fileConfig.chain });
        fileConfigFlags = `
                --config-path=${configPaths['general.yaml']}
                --contracts-config-path=${configPaths['contracts.yaml']}
                --secrets-path=${configPaths['secrets.yaml']}
                --wallets-path=${configPaths['wallets.yaml']}
                --genesis-path=${configPaths['genesis.yaml']}
            `;
    }

    const cmd = `cd ${pathToHome} && RUST_LOG=off cargo run --bin block_reverter --release -- ${args.join(
        ' '
    )} ${fileConfigFlags}`;
    const executedProcess = await exec(cmd, {
        cwd: env.ZKSYNC_HOME,
        env: {
            ...env,
            PATH: process.env.PATH
        }
    });

    return executedProcess.stdout;
}

async function killServerAndWaitForShutdown(proc: MainNode | ExtNode) {
    await proc.terminate();
    // Wait until it's really stopped.
    let iter = 0;
    while (iter < 30) {
        try {
            await proc.tester.syncWallet.provider.getBlockNumber();
            await utils.sleep(2);
            iter += 1;
        } catch (_) {
            // When exception happens, we assume that server died.
            return;
        }
    }
    // It's going to panic anyway, since the server is a singleton entity, so better to exit early.
    throw new Error("Server didn't stop after a kill request");
}

class MainNode {
    constructor(public tester: Tester, public proc: ChildProcessWithoutNullStreams, public zkInception: boolean) {}

    public async terminate() {
        try {
            // let data = await utils.exec(`pgrep -P ${this.proc.pid}`);
            // console.log('Pid: ', data.stdout);
            let parent = this.proc.pid;
            while (true) {
                try {
                    parent = +(await utils.exec(`pgrep -P ${parent}`)).stdout;
                    console.log('Parent stdout', parent);
                } catch (e) {
                    break;
                }
            }
            await utils.exec(`kill -9 ${parent}`);
        } catch (err) {
            console.log(`ignored error: ${err}`);
            // console.log(`ignored error: ${ err.stderr }`);
        }
    }

    // Terminates all main node processes running.
    public static async terminateAll() {
        try {
            await utils.exec('killall -INT zksync_server');
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    // Spawns a main node.
    // if enableConsensus is set, consensus component will be started in the main node.
    // if enableExecute is NOT set, main node will NOT send L1 transactions to execute L1 batches.
    public static async spawn(
        logs: fs.WriteStream,
        enableConsensus: boolean,
        enableExecute: boolean,
        ethClientWeb3Url: string,
        apiWeb3JsonRpcHttpUrl: string,
        baseTokenAddress: string
    ): Promise<MainNode> {
        let env = fetchEnv(mainEnv);
        env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = enableExecute ? '1' : '10000';
        // Set full mode for the Merkle tree as it is required to get blocks committed.
        env.DATABASE_MERKLE_TREE_MODE = 'full';

        if (fileConfig.loadFromFile) {
            replaceAggregatedBlockExecuteDeadline(pathToHome, fileConfig, enableExecute ? 1 : 10000);
        }

        let components = 'api,tree,eth,state_keeper,commitment_generator,da_dispatcher,vm_runner_protective_reads';
        if (enableConsensus) {
            components += ',consensus';
        }

        let proc = runServerInBackground({
            components: [components],
            stdio: [null, logs, logs],
            cwd: pathToHome,
            env: env,
            useZkInception: fileConfig.loadFromFile,
            chain: fileConfig.chain
        });

        // Wait until the main node starts responding.
        let tester: Tester = await Tester.init(ethClientWeb3Url, apiWeb3JsonRpcHttpUrl, baseTokenAddress);
        while (true) {
            try {
                await tester.syncWallet.provider.getBlockNumber();
                break;
            } catch (err) {
                if (proc.exitCode != null) {
                    assert.fail(`server failed to start, exitCode = ${proc.exitCode}`);
                }
                console.log('MainNode waiting for api endpoint');
                await utils.sleep(1);
            }
        }
        return new MainNode(tester, proc, fileConfig.loadFromFile);
    }
}

class ExtNode {
    constructor(public tester: Tester, private proc: child_process.ChildProcess, public zkInception: boolean) {}

    public async terminate() {
        try {
            // let data = await utils.exec(`pgrep -P ${this.proc.pid}`);
            // console.log('Pid: ', data.stdout);
            let parent = this.proc.pid;
            while (true) {
                try {
                    parent = +(await utils.exec(`pgrep -P ${parent}`)).stdout;
                    console.log('Parent stdout', parent);
                } catch (e) {
                    break;
                }
            }
            await utils.exec(`kill -9 ${parent}`);
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    // Terminates all main node processes running.
    public static async terminateAll() {
        try {
            await utils.exec('killall -INT zksync_external_node');
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    // Spawns an external node.
    // If enableConsensus is set, the node will use consensus P2P network to fetch blocks.
    public static async spawn(
        logs: fs.WriteStream,
        enableConsensus: boolean,
        ethClientWeb3Url: string,
        enEthClientUrl: string,
        baseTokenAddress: string
    ): Promise<ExtNode> {
        let env = fetchEnv(extEnv);
        let args = [];
        if (enableConsensus) {
            args.push('--enable-consensus');
        }

        // Run server in background.
        let proc = runExternalNodeInBackground({
            stdio: [null, logs, logs],
            cwd: pathToHome,
            env: env,
            useZkInception: fileConfig.loadFromFile,
            chain: fileConfig.chain
        });

        // Wait until the node starts responding.
        let tester: Tester = await Tester.init(ethClientWeb3Url, enEthClientUrl, baseTokenAddress);
        while (true) {
            try {
                await tester.syncWallet.provider.getBlockNumber();
                break;
            } catch (err) {
                if (proc.exitCode != null) {
                    assert.fail(`node failed to start, exitCode = ${proc.exitCode}`);
                }
                console.log('ExtNode waiting for api endpoint');
                await utils.sleep(1);
            }
        }
        return new ExtNode(tester, proc, fileConfig.loadFromFile);
    }

    // Waits for the node process to exit.
    public async waitForExit(): Promise<number> {
        while (this.proc.exitCode === null) {
            await utils.sleep(1);
        }
        return this.proc.exitCode;
    }
}

describe('Block reverting test', function () {
    let ethClientWeb3Url: string;
    let apiWeb3JsonRpcHttpUrl: string;
    let baseTokenAddress: string;
    let enEthClientUrl: string;
    let operatorAddress: string;
    let mainLogs: fs.WriteStream;
    let extLogs: fs.WriteStream;
    let depositAmount: bigint;
    let enableConsensus: boolean;

    before('initialize test', async () => {
        if (fileConfig.loadFromFile) {
            const secretsConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'secrets.yaml' });
            const generalConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'general.yaml' });
            const contractsConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'contracts.yaml' });
            const externalNodeGeneralConfig = loadConfig({
                pathToHome,
                configsFolderSuffix: 'external_node',
                chain: fileConfig.chain,
                config: 'general.yaml'
            });
            const walletsConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'wallets.yaml' });

            ethClientWeb3Url = secretsConfig.l1.l1_rpc_url;
            apiWeb3JsonRpcHttpUrl = generalConfig.api.web3_json_rpc.http_url;
            baseTokenAddress = contractsConfig.l1.base_token_addr;
            enEthClientUrl = externalNodeGeneralConfig.api.web3_json_rpc.http_url;
            operatorAddress = walletsConfig.operator.address;
            mainLogs = fs.createWriteStream(`${fileConfig.chain}_${mainLogsPath}`, { flags: 'a' });
            extLogs = fs.createWriteStream(`${fileConfig.chain}_${extLogs}`, { flags: 'a' });
        } else {
            let env = fetchEnv(mainEnv);
            ethClientWeb3Url = env.ETH_CLIENT_WEB3_URL;
            apiWeb3JsonRpcHttpUrl = env.API_WEB3_JSON_RPC_HTTP_URL;
            baseTokenAddress = env.CONTRACTS_BASE_TOKEN_ADDR;
            enEthClientUrl = `http://127.0.0.1:${env.EN_HTTP_PORT}`;
            // TODO use env variable for this?
            operatorAddress = '0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7';
            mainLogs = fs.createWriteStream(mainLogsPath, { flags: 'a' });
            extLogs = fs.createWriteStream(extLogsPath, { flags: 'a' });
        }

        if (process.env.SKIP_COMPILATION !== 'true' && !fileConfig.loadFromFile) {
            compileBinaries();
        }
        console.log(`PWD = ${process.env.PWD}`);
        enableConsensus = process.env.ENABLE_CONSENSUS === 'true';
        console.log(`enableConsensus = ${enableConsensus}`);
        depositAmount = ethers.parseEther('0.001');
    });

    step('run', async () => {
        console.log('Make sure that nodes are not running');
        await ExtNode.terminateAll();
        await MainNode.terminateAll();

        console.log('Start main node');
        let mainNode = await MainNode.spawn(
            mainLogs,
            enableConsensus,
            true,
            ethClientWeb3Url,
            apiWeb3JsonRpcHttpUrl,
            baseTokenAddress
        );
        console.log('Start ext node');
        let extNode = await ExtNode.spawn(extLogs, enableConsensus, ethClientWeb3Url, enEthClientUrl, baseTokenAddress);

        await mainNode.tester.fundSyncWallet();
        await extNode.tester.fundSyncWallet();

        const main_contract = await mainNode.tester.syncWallet.getMainContract();
        const baseToken = await mainNode.tester.syncWallet.getBaseToken();
        const isETHBasedChain = baseToken === zksync.utils.ETH_ADDRESS_IN_CONTRACTS;
        const alice: zksync.Wallet = extNode.tester.emptyWallet();

        console.log(
            'Finalize an L1 transaction to ensure at least 1 executed L1 batch and that all transactions are processed'
        );

        for (let iter = 0; iter < 30; iter++) {
            try {
                const h: zksync.types.PriorityOpResponse = await extNode.tester.syncWallet.deposit({
                    token: isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : baseToken,
                    amount: depositAmount,
                    to: alice.address,
                    approveBaseERC20: true,
                    approveERC20: true
                });
                await h.waitFinalize();
                break;
            } catch (error: any) {
                if (error.message == 'server shutting down') {
                    await utils.sleep(2);
                    continue;
                }
            }
        }

        console.log('Restart the main node with L1 batch execution disabled.');
        await mainNode.terminate();
        await killServerAndWaitForShutdown(mainNode);
        mainNode = await MainNode.spawn(
            mainLogs,
            enableConsensus,
            false,
            ethClientWeb3Url,
            apiWeb3JsonRpcHttpUrl,
            baseTokenAddress
        );

        console.log('Commit at least 2 L1 batches which are not executed');
        const lastExecuted = await main_contract.getTotalBatchesExecuted();
        // One is not enough to test the reversion of sk cache because
        // it gets updated with some batch logs only at the start of the next batch.
        const initialL1BatchNumber = await main_contract.getTotalBatchesCommitted();
        const firstDepositHandle = await extNode.tester.syncWallet.deposit({
            token: isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : baseToken,
            amount: depositAmount,
            to: alice.address,
            approveBaseERC20: true,
            approveERC20: true
        });

        await firstDepositHandle.wait();
        while ((await extNode.tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber) {
            await utils.sleep(0.1);
        }

        const secondDepositHandle = await extNode.tester.syncWallet.deposit({
            token: isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : baseToken,
            amount: depositAmount,
            to: alice.address,
            approveBaseERC20: true,
            approveERC20: true
        });
        await secondDepositHandle.wait();
        while ((await extNode.tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber + 1n) {
            await utils.sleep(0.3);
        }

        const alice2 = await alice.getBalance();
        while (true) {
            const lastCommitted = await main_contract.getTotalBatchesCommitted();
            console.log(`lastExecuted = ${lastExecuted}, lastCommitted = ${lastCommitted}`);
            if (lastCommitted - lastExecuted >= 2n) {
                console.log('Terminate the main node');
                await killServerAndWaitForShutdown(mainNode);
                break;
            }
            await utils.sleep(0.3);
        }

        console.log('Ask block_reverter to suggest to which L1 batch we should revert');
        const values_json = await runBlockReverter([
            'print-suggested-values',
            '--json',
            '--operator-address',
            operatorAddress
        ]);
        console.log(`values = ${values_json}`);
        const values = parseSuggestedValues(values_json);
        assert(lastExecuted === values.lastExecutedL1BatchNumber);

        console.log('Send reverting transaction to L1');
        await runBlockReverter([
            'send-eth-transaction',
            '--l1-batch-number',
            values.lastExecutedL1BatchNumber.toString(),
            '--nonce',
            values.nonce.toString(),
            '--priority-fee-per-gas',
            values.priorityFee.toString()
        ]);

        console.log('Check that batches are reverted on L1');
        const lastCommitted2 = await main_contract.getTotalBatchesCommitted();
        console.log(`lastCommitted = ${lastCommitted2}, want ${lastExecuted}`);
        assert(lastCommitted2 === lastExecuted);

        console.log('Rollback db');
        await runBlockReverter([
            'rollback-db',
            '--l1-batch-number',
            values.lastExecutedL1BatchNumber.toString(),
            '--rollback-postgres',
            '--rollback-tree',
            '--rollback-sk-cache',
            '--rollback-vm-runners-cache'
        ]);

        console.log('Start main node.');
        mainNode = await MainNode.spawn(
            mainLogs,
            enableConsensus,
            true,
            ethClientWeb3Url,
            apiWeb3JsonRpcHttpUrl,
            baseTokenAddress
        );

        console.log('Wait for the external node to detect reorg and terminate');
        await extNode.waitForExit();

        console.log('Restart external node and wait for it to revert.');
        extNode = await ExtNode.spawn(extLogs, enableConsensus, ethClientWeb3Url, enEthClientUrl, baseTokenAddress);

        console.log('Execute an L1 transaction');
        const depositHandle = await extNode.tester.syncWallet.deposit({
            token: isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : baseToken,
            amount: depositAmount,
            to: alice.address,
            approveBaseERC20: true,
            approveERC20: true
        });

        let l1TxResponse = await alice._providerL1().getTransaction(depositHandle.hash);
        while (!l1TxResponse) {
            console.log(`Deposit ${depositHandle.hash} is not visible to the L1 network; sleeping`);
            await utils.sleep(1);
            l1TxResponse = await alice._providerL1().getTransaction(depositHandle.hash);
        }

        // TODO: it would be nice to know WHY it "doesn't work well with block reversions" and what it actually means.
        console.log(
            "ethers doesn't work well with block reversions, so wait for the receipt before calling `.waitFinalize()`."
        );
        const l2Tx = await alice._providerL2().getL2TransactionFromPriorityOp(l1TxResponse);
        let receipt = null;
        while (true) {
            receipt = await extNode.tester.syncWallet.provider.getTransactionReceipt(l2Tx.hash);
            if (receipt != null) {
                break;
            }
            await utils.sleep(1);
        }
        await depositHandle.waitFinalize();
        expect(receipt.status).to.be.eql(1);

        // The reverted transactions are expected to be reexecuted before the next transaction is applied.
        // Hence we compare the state against the alice2, rather than against alice3.
        const alice4want = alice2 + depositAmount;
        const alice4 = await alice.getBalance();
        console.log(`Alice's balance is ${alice4}, want ${alice4want}`);
        assert(alice4 === alice4want);

        console.log('Execute an L2 transaction');
        await checkedRandomTransfer(alice, 1n);
    });

    after('terminate nodes', async () => {
        await MainNode.terminateAll();
        await ExtNode.terminateAll();

        if (fileConfig.loadFromFile) {
            replaceAggregatedBlockExecuteDeadline(pathToHome, fileConfig, 10);
        }
    });
});

// Transfers amount from sender to a random wallet in an L2 transaction.
async function checkedRandomTransfer(sender: zksync.Wallet, amount: bigint) {
    const senderBalanceBefore = await sender.getBalance();
    const receiver = zksync.Wallet.createRandom().connect(sender.provider);
    const transferHandle = await sender.sendTransaction({ to: receiver.address, value: amount, type: 0 });

    // ethers doesn't work well with block reversions, so we poll for the receipt manually.
    let txReceipt = null;
    do {
        txReceipt = await sender.provider.getTransactionReceipt(transferHandle.hash);
        await utils.sleep(1);
    } while (txReceipt === null);

    const senderBalance = await sender.getBalance();
    const receiverBalance = await receiver.provider!.getBalance(receiver.address);

    expect(receiverBalance === amount, 'Failed updated the balance of the receiver').to.be.true;

    const spentAmount = txReceipt.gasUsed * transferHandle.gasPrice! + amount;
    expect(senderBalance + spentAmount >= senderBalanceBefore, 'Failed to update the balance of the sender').to.be.true;
}

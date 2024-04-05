import * as utils from 'zk/build/utils';
import { Tester } from './tester';
import * as zkweb3 from 'zksync-ethers';
import { BigNumber, BigNumberish, ethers } from 'ethers';
import { expect } from 'chai';
import { hashBytecode } from 'zksync-web3/build/src/utils';
import fs from 'fs';
import { TransactionResponse } from 'zksync-web3/build/src/types';
import { BytesLike } from '@ethersproject/bytes';

const L1_CONTRACTS_FOLDER = `${process.env.ZKSYNC_HOME}/contracts/l1-contracts/artifacts/contracts`;
const L1_DEFAULT_UPGRADE_ABI = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER}/upgrades/DefaultUpgrade.sol/DefaultUpgrade.json`).abi
);
const GOVERNANCE_ABI = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER}/governance/Governance.sol/Governance.json`).abi
);
const ADMIN_FACET_ABI = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER}/state-transition/chain-interfaces/IAdmin.sol/IAdmin.json`).abi
);
const L2_FORCE_DEPLOY_UPGRADER_ABI = new ethers.utils.Interface(
    require(`${process.env.ZKSYNC_HOME}/contracts/l2-contracts/artifacts-zk/cache-zk/solpp-generated-contracts/ForceDeployUpgrader.sol/ForceDeployUpgrader.json`).abi
);
const COMPLEX_UPGRADER_ABI = new ethers.utils.Interface(
    require(`${process.env.ZKSYNC_HOME}/contracts/system-contracts/artifacts-zk/contracts-preprocessed/ComplexUpgrader.sol/ComplexUpgrader.json`).abi
);
const COUNTER_BYTECODE =
    require(`${process.env.ZKSYNC_HOME}/core/tests/ts-integration/artifacts-zk/contracts/counter/counter.sol/Counter.json`).deployedBytecode;
const STATE_TRANSITON_MANAGER = new ethers.utils.Interface(
    require(`${L1_CONTRACTS_FOLDER}/state-transition/StateTransitionManager.sol/StateTransitionManager.json`).abi
);

const depositAmount = ethers.utils.parseEther('0.001');

describe('Upgrade test', function () {
    let tester: Tester;
    let alice: zkweb3.Wallet;
    let govWallet: ethers.Wallet;
    let mainContract: ethers.Contract;
    let governanceContract: ethers.Contract;
    let bootloaderHash: string;
    let scheduleTransparentOperation: string;
    let executeOperation: string;
    let finalizeOperation: string;
    let forceDeployAddress: string;
    let forceDeployBytecode: string;
    let logs: fs.WriteStream;

    before('Create test wallet', async () => {
        tester = await Tester.init(process.env.CHAIN_ETH_NETWORK || 'localhost');
        alice = tester.emptyWallet();
        logs = fs.createWriteStream('upgrade.log', { flags: 'a' });

        const govMnemonic = require('../../../../etc/test_config/constant/eth.json').mnemonic;
        govWallet = ethers.Wallet.fromMnemonic(govMnemonic, "m/44'/60'/0'/0/1").connect(alice._providerL1());
    });

    step('Run server and execute some transactions', async () => {
        // Make sure server isn't running.
        try {
            await utils.exec('pkill zksync_server');
            // It may take some time for witness generator to stop.
            await utils.sleep(10);
        } catch (_) {}

        // Set small timeouts.
        process.env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_COMMIT_DEADLINE = '1';
        process.env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_PROVE_DEADLINE = '1';
        process.env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = '1';
        // Must be > 1s, because bootloader requires l1 batch timestamps to be incremental.
        process.env.CHAIN_STATE_KEEPER_BLOCK_COMMIT_DEADLINE_MS = '2000';
        // Run server in background.
        utils.background(
            'cd $ZKSYNC_HOME && cargo run --bin zksync_server --release -- --components=api,tree,eth,state_keeper,commitment_generator',
            [null, logs, logs]
        );
        // Server may need some time to recompile if it's a cold run, so wait for it.
        let iter = 0;
        while (iter < 30 && !mainContract) {
            try {
                mainContract = await tester.syncWallet.getMainContract();
            } catch (_) {
                await utils.sleep(1);
            }
            iter += 1;
        }
        if (!mainContract) {
            throw new Error('Server did not start');
        }
        const stmAddr = await mainContract.getStateTransitionManager();
        const stmContract = new ethers.Contract(stmAddr, STATE_TRANSITON_MANAGER, tester.syncWallet.providerL1);
        const governanceAddr = await stmContract.owner();
        governanceContract = new ethers.Contract(governanceAddr, GOVERNANCE_ABI, tester.syncWallet.providerL1);
        let blocksCommitted = await mainContract.getTotalBatchesCommitted();

        const initialL1BatchNumber = await tester.web3Provider.getL1BatchNumber();

        const firstDepositHandle = await tester.syncWallet.deposit({
            token: zkweb3.utils.ETH_ADDRESS,
            amount: depositAmount,
            to: alice.address
        });
        await firstDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber) {
            await utils.sleep(1);
        }

        const secondDepositHandle = await tester.syncWallet.deposit({
            token: zkweb3.utils.ETH_ADDRESS,
            amount: depositAmount,
            to: alice.address
        });
        await secondDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber + 1) {
            await utils.sleep(1);
        }

        const balance = await alice.getBalance();
        expect(balance.eq(depositAmount.mul(2)), 'Incorrect balance after deposits').to.be.true;

        if (process.env.CHECK_EN_URL) {
            console.log('Checking EN after deposit');
            await utils.sleep(2);
            const enProvider = new ethers.providers.JsonRpcProvider(process.env.CHECK_EN_URL);
            const enBalance = await enProvider.getBalance(alice.address);
            expect(enBalance.eq(balance), 'Failed to update the balance on EN after deposit').to.be.true;
        }

        // Wait for at least one new committed block
        let newBlocksCommitted = await mainContract.getTotalBatchesCommitted();
        let tryCount = 0;
        while (blocksCommitted.eq(newBlocksCommitted) && tryCount < 10) {
            newBlocksCommitted = await mainContract.getTotalBatchesCommitted();
            tryCount += 1;
            await utils.sleep(1);
        }
    });

    step('Send l1 tx for saving new bootloader', async () => {
        const path = `${process.env.ZKSYNC_HOME}/contracts/system-contracts/bootloader/build/artifacts/playground_batch.yul.zbin`;
        const bootloaderCode = ethers.utils.hexlify(fs.readFileSync(path));
        bootloaderHash = ethers.utils.hexlify(hashBytecode(bootloaderCode));
        const txHandle = await tester.syncWallet.requestExecute({
            contractAddress: ethers.constants.AddressZero,
            calldata: '0x',
            l2GasLimit: 20000000,
            factoryDeps: [bootloaderCode],
            overrides: {
                gasLimit: 3000000
            }
        });
        await txHandle.wait();
        await waitForNewL1Batch(alice);
    });

    step('Schedule governance call', async () => {
        forceDeployAddress = '0xf04ce00000000000000000000000000000000000';
        forceDeployBytecode = COUNTER_BYTECODE;

        const forceDeployment: ForceDeployment = {
            bytecodeHash: hashBytecode(forceDeployBytecode),
            newAddress: forceDeployAddress,
            callConstructor: false,
            value: BigNumber.from(0),
            input: '0x'
        };

        const delegateTo = process.env.CONTRACTS_L2_DEFAULT_UPGRADE_ADDR!;
        const delegateCalldata = L2_FORCE_DEPLOY_UPGRADER_ABI.encodeFunctionData('forceDeploy', [[forceDeployment]]);
        const data = COMPLEX_UPGRADER_ABI.encodeFunctionData('upgrade', [delegateTo, delegateCalldata]);

        const oldProtocolVersion = await alice._providerL2().send('zks_getProtocolVersion', [null]);
        const calldata = await prepareUpgradeCalldata(govWallet, alice._providerL2(), {
            l2ProtocolUpgradeTx: {
                txType: 254,
                from: '0x0000000000000000000000000000000000008007', // FORCE_DEPLOYER address
                to: '0x000000000000000000000000000000000000800f', // ComplexUpgrader address
                gasLimit: process.env.CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT!,
                gasPerPubdataByteLimit: zkweb3.utils.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT,
                maxFeePerGas: 0,
                maxPriorityFeePerGas: 0,
                paymaster: 0,
                value: 0,
                reserved: [0, 0, 0, 0],
                data,
                signature: '0x',
                factoryDeps: [hashBytecode(forceDeployBytecode)],
                paymasterInput: '0x',
                reservedDynamic: '0x'
            },
            factoryDeps: [forceDeployBytecode],
            bootloaderHash,
            upgradeTimestamp: 0,
            oldProtocolVersion: oldProtocolVersion.version_id
        });
        scheduleTransparentOperation = calldata.scheduleTransparentOperation;
        executeOperation = calldata.executeOperation;
        finalizeOperation = calldata.finalizeOperation;

        const scheduleUpgrade = await govWallet.sendTransaction({
            to: governanceContract.address,
            data: scheduleTransparentOperation
        });
        await scheduleUpgrade.wait();

        // Wait for server to process L1 event.
        await utils.sleep(2);
    });

    step('Check bootloader is updated on L2', async () => {
        const receipt = await waitForNewL1Batch(alice);
        const batchDetails = await alice.provider.getL1BatchDetails(receipt.l1BatchNumber);
        expect(batchDetails.baseSystemContractsHashes.bootloader).to.eq(bootloaderHash);
    });

    step('Execute upgrade', async () => {
        // Wait for batches with old bootloader to be executed on L1.
        let l1BatchNumber = await alice.provider.getL1BatchNumber();
        while (
            (await alice.provider.getL1BatchDetails(l1BatchNumber)).baseSystemContractsHashes.bootloader ==
            bootloaderHash
        ) {
            l1BatchNumber -= 1;
        }

        let lastBatchExecuted = await mainContract.getTotalBatchesExecuted();
        let tryCount = 0;
        while (lastBatchExecuted < l1BatchNumber && tryCount < 10) {
            lastBatchExecuted = await mainContract.getTotalBatchesExecuted();
            tryCount += 1;
            await utils.sleep(2);
        }
        if (lastBatchExecuted < l1BatchNumber) {
            throw new Error('Server did not execute old blocks');
        }

        // Send execute tx.
        const execute = await govWallet.sendTransaction({
            to: governanceContract.address,
            data: executeOperation
        });
        await execute.wait();
    });

    step('Finalize upgrade on the target chain', async () => {
        // Send finalize tx.
        const finalize = await govWallet.sendTransaction({
            to: mainContract.address,
            data: finalizeOperation
        });
        await finalize.wait();

        let bootloaderHashL1 = await mainContract.getL2BootloaderBytecodeHash();
        expect(bootloaderHashL1).eq(bootloaderHash);
    });

    step('Wait for block finalization', async () => {
        // Execute an L2 transaction
        const txHandle = await checkedRandomTransfer(alice, BigNumber.from(1));
        await txHandle.waitFinalize();
    });

    step('Check force deploy', async () => {
        const deployedCode = await alice.provider.getCode(forceDeployAddress);
        expect(deployedCode.toLowerCase()).eq(forceDeployBytecode.toLowerCase());
    });

    step('Execute transactions after simple restart', async () => {
        // Stop server.
        await utils.exec('pkill zksync_server');
        await utils.sleep(5);

        // Run again.
        utils.background(
            'cd $ZKSYNC_HOME && cargo run --bin zksync_server --release -- --components=api,tree,eth,state_keeper,commitment_generator &> upgrade.log',
            [null, logs, logs]
        );
        await utils.sleep(5);

        // Trying to send a transaction from the same address again
        await checkedRandomTransfer(alice, BigNumber.from(1));
    });

    after('Try killing server', async () => {
        try {
            await utils.exec('pkill zksync_server');
        } catch (_) {}
    });
});

async function checkedRandomTransfer(sender: zkweb3.Wallet, amount: BigNumber): Promise<TransactionResponse> {
    const senderBalanceBefore = await sender.getBalance();
    const receiver = zkweb3.Wallet.createRandom().connect(sender.provider);
    const transferHandle = await sender.sendTransaction({
        to: receiver.address,
        value: amount
    });
    const txReceipt = await transferHandle.wait();

    const senderBalanceAfter = await sender.getBalance();
    const receiverBalanceAfter = await receiver.getBalance();

    expect(receiverBalanceAfter.eq(amount), 'Failed updated the balance of the receiver').to.be.true;

    const spentAmount = txReceipt.gasUsed.mul(transferHandle.gasPrice!).add(amount);
    expect(senderBalanceAfter.add(spentAmount).gte(senderBalanceBefore), 'Failed to update the balance of the sender')
        .to.be.true;

    if (process.env.CHECK_EN_URL) {
        console.log('Checking EN after transfer');
        await utils.sleep(2);
        const enProvider = new ethers.providers.JsonRpcProvider(process.env.CHECK_EN_URL);
        const enSenderBalance = await enProvider.getBalance(sender.address);
        expect(enSenderBalance.eq(senderBalanceAfter), 'Failed to update the balance of the sender on EN').to.be.true;
    }

    return transferHandle;
}

interface ForceDeployment {
    // The bytecode hash to put on an address
    bytecodeHash: BytesLike;
    // The address on which to deploy the bytecodehash to
    newAddress: string;
    // Whether to call the constructor
    callConstructor: boolean;
    // The value with which to initialize a contract
    value: BigNumber;
    // The constructor calldata
    input: BytesLike;
}

async function waitForNewL1Batch(wallet: zkweb3.Wallet): Promise<zkweb3.types.TransactionReceipt> {
    // Send a dummy transaction and wait until the new L1 batch is created.
    const oldReceipt = await wallet.transfer({ to: wallet.address, amount: 0 }).then((tx) => tx.wait());
    // Invariant: even with 1 transaction, l1 batch must be eventually sealed, so this loop must exit.
    while (!(await wallet.provider.getTransactionReceipt(oldReceipt.transactionHash)).l1BatchNumber) {
        await zkweb3.utils.sleep(wallet.provider.pollingInterval);
    }
    return await wallet.provider.getTransactionReceipt(oldReceipt.transactionHash);
}

async function prepareUpgradeCalldata(
    govWallet: ethers.Wallet,
    l2Provider: zkweb3.Provider,
    params: {
        l2ProtocolUpgradeTx: {
            txType: BigNumberish;
            from: BigNumberish;
            to: BigNumberish;
            gasLimit: BigNumberish;
            gasPerPubdataByteLimit: BigNumberish;
            maxFeePerGas: BigNumberish;
            maxPriorityFeePerGas: BigNumberish;
            paymaster: BigNumberish;
            nonce?: BigNumberish;
            value: BigNumberish;
            reserved: [BigNumberish, BigNumberish, BigNumberish, BigNumberish];
            data: BytesLike;
            signature: BytesLike;
            factoryDeps: BigNumberish[];
            paymasterInput: BytesLike;
            reservedDynamic: BytesLike;
        };
        factoryDeps: BytesLike[];
        bootloaderHash?: BytesLike;
        defaultAAHash?: BytesLike;
        verifier?: string;
        verifierParams?: {
            recursionNodeLevelVkHash: BytesLike;
            recursionLeafLevelVkHash: BytesLike;
            recursionCircuitsSetVksHash: BytesLike;
        };
        l1ContractsUpgradeCalldata?: BytesLike;
        postUpgradeCalldata?: BytesLike;
        upgradeTimestamp: BigNumberish;
        oldProtocolVersion?: BigNumberish;
    }
) {
    const upgradeAddress = process.env.CONTRACTS_DEFAULT_UPGRADE_ADDR;

    if (!upgradeAddress) {
        throw new Error('CONTRACTS_DEFAULT_UPGRADE_ADDR not set');
    }

    const zksyncAddress = await l2Provider.getMainContractAddress();
    const zksync = new ethers.Contract(zksyncAddress, zkweb3.utils.ZKSYNC_MAIN_ABI, govWallet);
    const stmAddress = await zksync.getStateTransitionManager();

    const oldProtocolVersion = params.oldProtocolVersion ?? (await zksync.getProtocolVersion());
    const newProtocolVersion = ethers.BigNumber.from(oldProtocolVersion).add(1);
    params.l2ProtocolUpgradeTx.nonce ??= newProtocolVersion;
    const upgradeInitData = L1_DEFAULT_UPGRADE_ABI.encodeFunctionData('upgrade', [
        [
            params.l2ProtocolUpgradeTx,
            params.factoryDeps,
            params.bootloaderHash ?? ethers.constants.HashZero,
            params.defaultAAHash ?? ethers.constants.HashZero,
            params.verifier ?? ethers.constants.AddressZero,
            params.verifierParams ?? [ethers.constants.HashZero, ethers.constants.HashZero, ethers.constants.HashZero],
            params.l1ContractsUpgradeCalldata ?? '0x',
            params.postUpgradeCalldata ?? '0x',
            params.upgradeTimestamp,
            newProtocolVersion
        ]
    ]);

    // Prepare the diamond cut data
    const upgradeParam = {
        facetCuts: [],
        initAddress: upgradeAddress,
        initCalldata: upgradeInitData
    };

    // Prepare calldata for upgrading STM
    const stmUpgradeCalldata = STATE_TRANSITON_MANAGER.encodeFunctionData('setNewVersionUpgrade', [
        upgradeParam,
        oldProtocolVersion,
        newProtocolVersion
    ]);

    const call = {
        target: stmAddress,
        value: 0,
        data: stmUpgradeCalldata
    };
    const governanceOperation = {
        calls: [call],
        predecessor: ethers.constants.HashZero,
        salt: ethers.constants.HashZero
    };

    // Get transaction data of the `scheduleTransparent`
    const scheduleTransparentOperation = GOVERNANCE_ABI.encodeFunctionData('scheduleTransparent', [
        governanceOperation,
        0 // delay
    ]);

    // Get transaction data of the `execute`
    const executeOperation = GOVERNANCE_ABI.encodeFunctionData('execute', [governanceOperation]);

    // Execute this upgrade on a specific chain under this STM.
    const finalizeOperation = ADMIN_FACET_ABI.encodeFunctionData('upgradeChainFromVersion', [
        oldProtocolVersion,
        upgradeParam
    ]);

    return {
        scheduleTransparentOperation,
        executeOperation,
        finalizeOperation
    };
}

import * as utils from 'utils';
import { Tester } from './tester';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect } from 'chai';
import fs from 'fs';
import { BytesLike } from '@ethersproject/bytes';
import { IZkSyncHyperchain } from 'zksync-ethers/build/typechain';
import { BigNumberish } from 'ethers';

const L1_CONTRACTS_FOLDER = `${process.env.ZKSYNC_HOME}/contracts/l1-contracts/artifacts/contracts`;
const L1_DEFAULT_UPGRADE_ABI = new ethers.Interface(
    require(`${L1_CONTRACTS_FOLDER}/upgrades/DefaultUpgrade.sol/DefaultUpgrade.json`).abi
);
const GOVERNANCE_ABI = new ethers.Interface(
    require(`${L1_CONTRACTS_FOLDER}/governance/Governance.sol/Governance.json`).abi
);
const ADMIN_FACET_ABI = new ethers.Interface(
    require(`${L1_CONTRACTS_FOLDER}/state-transition/chain-interfaces/IAdmin.sol/IAdmin.json`).abi
);
const L2_FORCE_DEPLOY_UPGRADER_ABI = new ethers.Interface(
    require(`${process.env.ZKSYNC_HOME}/contracts/l2-contracts/artifacts-zk/contracts/ForceDeployUpgrader.sol/ForceDeployUpgrader.json`).abi
);
const COMPLEX_UPGRADER_ABI = new ethers.Interface(
    require(`${process.env.ZKSYNC_HOME}/contracts/system-contracts/artifacts-zk/contracts-preprocessed/ComplexUpgrader.sol/ComplexUpgrader.json`).abi
);
const COUNTER_BYTECODE =
    require(`${process.env.ZKSYNC_HOME}/core/tests/ts-integration/artifacts-zk/contracts/counter/counter.sol/Counter.json`).deployedBytecode;
const STATE_TRANSITON_MANAGER = new ethers.Interface(
    require(`${L1_CONTRACTS_FOLDER}/state-transition/StateTransitionManager.sol/StateTransitionManager.json`).abi
);

let serverComponents = 'api,tree,eth,state_keeper,commitment_generator,da_dispatcher';

const depositAmount = ethers.parseEther('0.001');

describe('Upgrade test', function () {
    let tester: Tester;
    let alice: zksync.Wallet;
    let govWallet: ethers.Wallet;
    let mainContract: IZkSyncHyperchain;
    let governanceContract: ethers.Contract;
    let bootloaderHash: string;
    let scheduleTransparentOperation: string;
    let executeOperation: string;
    let forceDeployAddress: string;
    let forceDeployBytecode: string;
    let logs: fs.WriteStream;

    before('Create test wallet', async () => {
        tester = await Tester.init(process.env.CHAIN_ETH_NETWORK || 'localhost');
        alice = tester.emptyWallet();
        logs = fs.createWriteStream('upgrade.log', { flags: 'a' });

        const govMnemonic = ethers.Mnemonic.fromPhrase(
            require('../../../../etc/test_config/constant/eth.json').mnemonic
        );
        const govWalletHD = ethers.HDNodeWallet.fromMnemonic(govMnemonic, "m/44'/60'/0'/0/1");
        govWallet = new ethers.Wallet(govWalletHD.privateKey, alice._providerL1());
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
        utils.background({
            command: `cd $ZKSYNC_HOME && cargo run --bin zksync_server --release -- --components=${serverComponents}`,
            stdio: [null, logs, logs]
        });
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

        const baseToken = await tester.syncWallet.provider.getBaseTokenContractAddress();
        console.log('Base token address:', baseToken);

        if (!zksync.utils.isAddressEq(baseToken, zksync.utils.ETH_ADDRESS_IN_CONTRACTS)) {
            console.log('Approving ERC20 token');
            await (await tester.syncWallet.approveERC20(baseToken, ethers.MaxUint256)).wait();
            console.log('Minting ERC20 token');
            console.log('balance:', await tester.syncWallet.getBalance(baseToken, 'committed'));
            await mintToWallet(baseToken, tester.syncWallet, depositAmount * 10n);
            console.log('balance:', await tester.syncWallet.getBalance(baseToken, 'committed'));
        }

        const firstDepositHandle = await tester.syncWallet.deposit({
            token: baseToken,
            amount: depositAmount,
            to: alice.address
        });
        await firstDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber) {
            await utils.sleep(1);
        }

        const secondDepositHandle = await tester.syncWallet.deposit({
            token: baseToken,
            amount: depositAmount,
            to: alice.address
        });
        await secondDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber + 1) {
            await utils.sleep(1);
        }

        const balance = await alice.getBalance();
        expect(balance === depositAmount * 2n, 'Incorrect balance after deposits').to.be.true;

        if (process.env.CHECK_EN_URL) {
            console.log('Checking EN after deposit');
            await utils.sleep(2);
            const enProvider = new ethers.JsonRpcProvider(process.env.CHECK_EN_URL);
            const enBalance = await enProvider.getBalance(alice.address);
            expect(enBalance === balance, 'Failed to update the balance on EN after deposit').to.be.true;
        }

        // Wait for at least one new committed block
        let newBlocksCommitted = await mainContract.getTotalBatchesCommitted();
        let tryCount = 0;
        while (blocksCommitted === newBlocksCommitted && tryCount < 30) {
            newBlocksCommitted = await mainContract.getTotalBatchesCommitted();
            tryCount += 1;
            await utils.sleep(1);
        }
    });

    step('Send l1 tx for saving new bootloader', async () => {
        const path = `${process.env.ZKSYNC_HOME}/contracts/system-contracts/bootloader/build/artifacts/playground_batch.yul.zbin`;
        const bootloaderCode = ethers.hexlify(fs.readFileSync(path));
        bootloaderHash = ethers.hexlify(zksync.utils.hashBytecode(bootloaderCode));
        const txHandle = await tester.syncWallet.requestExecute({
            contractAddress: ethers.ZeroAddress,
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
            bytecodeHash: ethers.hexlify(zksync.utils.hashBytecode(forceDeployBytecode)),
            newAddress: forceDeployAddress,
            callConstructor: false,
            value: 0n,
            input: '0x'
        };

        const delegateTo = process.env.CONTRACTS_L2_DEFAULT_UPGRADE_ADDR!;
        const delegateCalldata = L2_FORCE_DEPLOY_UPGRADER_ABI.encodeFunctionData('forceDeploy', [[forceDeployment]]);
        const data = COMPLEX_UPGRADER_ABI.encodeFunctionData('upgrade', [delegateTo, delegateCalldata]);

        const { stmUpgradeData, chainUpgradeData } = await prepareUpgradeCalldata(
            govWallet,
            alice._providerL2(),
            await mainContract.getAddress(),
            {
                l2ProtocolUpgradeTx: {
                    txType: 254,
                    from: '0x0000000000000000000000000000000000008007', // FORCE_DEPLOYER address
                    to: '0x000000000000000000000000000000000000800f', // ComplexUpgrader address
                    gasLimit: process.env.CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT!,
                    gasPerPubdataByteLimit: zksync.utils.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT,
                    maxFeePerGas: 0,
                    maxPriorityFeePerGas: 0,
                    paymaster: 0,
                    value: 0,
                    reserved: [0, 0, 0, 0],
                    data,
                    signature: '0x',
                    factoryDeps: [ethers.hexlify(zksync.utils.hashBytecode(forceDeployBytecode))],
                    paymasterInput: '0x',
                    reservedDynamic: '0x'
                },
                factoryDeps: [forceDeployBytecode],
                bootloaderHash,
                upgradeTimestamp: 0
            }
        );
        scheduleTransparentOperation = chainUpgradeData.scheduleTransparentOperation;
        executeOperation = chainUpgradeData.executeOperation;

        await sendGovernanceOperation(stmUpgradeData.scheduleTransparentOperation);
        await sendGovernanceOperation(stmUpgradeData.executeOperation);
        await sendGovernanceOperation(scheduleTransparentOperation);

        // Wait for server to process L1 event.
        await utils.sleep(2);
    });

    step('Check bootloader is updated on L2', async () => {
        const receipt = await waitForNewL1Batch(alice);
        const batchDetails = await alice.provider.getL1BatchDetails(receipt.l1BatchNumber!);
        expect(batchDetails.baseSystemContractsHashes.bootloader).to.eq(bootloaderHash);
    });

    step('Finalize upgrade on the target chain', async () => {
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
        while (lastBatchExecuted < l1BatchNumber && tryCount < 40) {
            lastBatchExecuted = await mainContract.getTotalBatchesExecuted();
            tryCount += 1;
            await utils.sleep(2);
        }
        if (lastBatchExecuted < l1BatchNumber) {
            throw new Error('Server did not execute old blocks');
        }

        // Execute the upgrade
        await sendGovernanceOperation(executeOperation);

        let bootloaderHashL1 = await mainContract.getL2BootloaderBytecodeHash();
        expect(bootloaderHashL1).eq(bootloaderHash);
    });

    step('Wait for block finalization', async () => {
        // Execute an L2 transaction
        const txHandle = await checkedRandomTransfer(alice, 1n);
        await txHandle.waitFinalize();
    });

    step('Check force deploy', async () => {
        const deployedCode = await alice.provider.getCode(forceDeployAddress);
        expect(deployedCode.toLowerCase()).eq(forceDeployBytecode.toLowerCase());
    });

    step('Execute transactions after simple restart', async () => {
        // Stop server.
        await utils.exec('pkill zksync_server');
        await utils.sleep(10);

        // Run again.
        utils.background({
            command: `cd $ZKSYNC_HOME && zk f cargo run --bin zksync_server --release -- --components=${serverComponents} &> upgrade.log`,
            stdio: [null, logs, logs]
        });
        await utils.sleep(10);

        // Trying to send a transaction from the same address again
        await checkedRandomTransfer(alice, 1n);
    });

    after('Try killing server', async () => {
        try {
            await utils.exec('pkill zksync_server');
        } catch (_) {}
    });

    async function sendGovernanceOperation(data: string) {
        await (
            await govWallet.sendTransaction({
                to: await governanceContract.getAddress(),
                data: data,
                type: 0
            })
        ).wait();
    }
});

async function checkedRandomTransfer(sender: zksync.Wallet, amount: bigint): Promise<zksync.types.TransactionResponse> {
    const senderBalanceBefore = await sender.getBalance();
    const receiverHD = zksync.Wallet.createRandom();
    const receiver = new zksync.Wallet(receiverHD.privateKey, sender.provider);
    const transferHandle = await sender.sendTransaction({
        to: receiver.address,
        value: amount,
        type: 0
    });
    const txReceipt = await transferHandle.wait();

    const senderBalanceAfter = await sender.getBalance();
    const receiverBalanceAfter = await receiver.getBalance();

    expect(receiverBalanceAfter === amount, 'Failed updated the balance of the receiver').to.be.true;

    const spentAmount = txReceipt.gasUsed * transferHandle.gasPrice! + amount;
    expect(senderBalanceAfter + spentAmount >= senderBalanceBefore, 'Failed to update the balance of the sender').to.be
        .true;

    if (process.env.CHECK_EN_URL) {
        console.log('Checking EN after transfer');
        await utils.sleep(2);
        const enProvider = new ethers.JsonRpcProvider(process.env.CHECK_EN_URL);
        const enSenderBalance = await enProvider.getBalance(sender.address);
        expect(enSenderBalance === senderBalanceAfter, 'Failed to update the balance of the sender on EN').to.be.true;
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
    value: bigint;
    // The constructor calldata
    input: BytesLike;
}

async function waitForNewL1Batch(wallet: zksync.Wallet): Promise<zksync.types.TransactionReceipt> {
    // Send a dummy transaction and wait until the new L1 batch is created.
    const oldReceipt = await wallet.transfer({ to: wallet.address, amount: 0 }).then((tx) => tx.wait());
    // Invariant: even with 1 transaction, l1 batch must be eventually sealed, so this loop must exit.
    while (!(await wallet.provider.getTransactionReceipt(oldReceipt.hash))!.l1BatchNumber) {
        await zksync.utils.sleep(wallet.provider.pollingInterval);
    }
    const receipt = await wallet.provider.getTransactionReceipt(oldReceipt.hash);
    if (!receipt) {
        throw new Error('Failed to get the receipt of the transaction');
    }
    return receipt;
}

async function prepareUpgradeCalldata(
    govWallet: ethers.Wallet,
    l2Provider: zksync.Provider,
    mainContract: zksync.types.Address,
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
    }
) {
    const upgradeAddress = process.env.CONTRACTS_DEFAULT_UPGRADE_ADDR;

    if (!upgradeAddress) {
        throw new Error('CONTRACTS_DEFAULT_UPGRADE_ADDR not set');
    }

    const zksyncAddress = await l2Provider.getMainContractAddress();
    const zksyncContract = new ethers.Contract(zksyncAddress, zksync.utils.ZKSYNC_MAIN_ABI, govWallet);
    const stmAddress = await zksyncContract.getStateTransitionManager();

    const oldProtocolVersion = Number(await zksyncContract.getProtocolVersion());
    const newProtocolVersion = addToProtocolVersion(oldProtocolVersion, 1, 1);

    params.l2ProtocolUpgradeTx.nonce ??= BigInt(unpackNumberSemVer(newProtocolVersion)[1]);
    const upgradeInitData = L1_DEFAULT_UPGRADE_ABI.encodeFunctionData('upgrade', [
        [
            params.l2ProtocolUpgradeTx,
            params.factoryDeps,
            params.bootloaderHash ?? ethers.ZeroHash,
            params.defaultAAHash ?? ethers.ZeroHash,
            params.verifier ?? ethers.ZeroAddress,
            params.verifierParams ?? [ethers.ZeroHash, ethers.ZeroHash, ethers.ZeroHash],
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
        // The protocol version will not have any deadline in this upgrade
        ethers.MaxUint256,
        newProtocolVersion
    ]);

    // Execute this upgrade on a specific chain under this STM.
    const chainUpgradeCalldata = ADMIN_FACET_ABI.encodeFunctionData('upgradeChainFromVersion', [
        oldProtocolVersion,
        upgradeParam
    ]);

    const stmUpgradeData = prepareGovernanceCalldata(stmAddress, stmUpgradeCalldata);
    const chainUpgradeData = prepareGovernanceCalldata(mainContract, chainUpgradeCalldata);

    return {
        chainUpgradeData,
        stmUpgradeData
    };
}

interface UpgradeCalldata {
    scheduleTransparentOperation: string;
    executeOperation: string;
}

function prepareGovernanceCalldata(to: string, data: BytesLike): UpgradeCalldata {
    const call = {
        target: to,
        value: 0,
        data
    };
    const governanceOperation = {
        calls: [call],
        predecessor: ethers.ZeroHash,
        salt: ethers.ZeroHash
    };

    // Get transaction data of the `scheduleTransparent`
    const scheduleTransparentOperation = GOVERNANCE_ABI.encodeFunctionData('scheduleTransparent', [
        governanceOperation,
        0 // delay
    ]);

    // Get transaction data of the `execute`
    const executeOperation = GOVERNANCE_ABI.encodeFunctionData('execute', [governanceOperation]);

    return {
        scheduleTransparentOperation,
        executeOperation
    };
}

async function mintToWallet(baseTokenAddress: zksync.types.Address, ethersWallet: ethers.Wallet, amountToMint: bigint) {
    const l1Erc20ABI = ['function mint(address to, uint256 amount)'];
    const l1Erc20Contract = new ethers.Contract(baseTokenAddress, l1Erc20ABI, ethersWallet);
    await (await l1Erc20Contract.mint(ethersWallet.address, amountToMint)).wait();
}

const SEMVER_MINOR_VERSION_MULTIPLIER = 4294967296;

function unpackNumberSemVer(semver: number): [number, number, number] {
    const major = 0;
    const minor = Math.floor(semver / SEMVER_MINOR_VERSION_MULTIPLIER);
    const patch = semver % SEMVER_MINOR_VERSION_MULTIPLIER;
    return [major, minor, patch];
}

// The major version is always 0 for now
export function packSemver(major: number, minor: number, patch: number) {
    if (major !== 0) {
        throw new Error('Major version must be 0');
    }

    return minor * SEMVER_MINOR_VERSION_MULTIPLIER + patch;
}

export function addToProtocolVersion(packedProtocolVersion: number, minor: number, patch: number) {
    const [major, minorVersion, patchVersion] = unpackNumberSemVer(packedProtocolVersion);
    return packSemver(major, minorVersion + minor, patchVersion + patch);
}

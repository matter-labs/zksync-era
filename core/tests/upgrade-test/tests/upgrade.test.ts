import * as utils from 'utils';
import { Tester } from './tester';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect } from 'chai';
import fs from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import { BytesLike } from '@ethersproject/bytes';
import { BigNumberish } from 'ethers';
import { loadConfig, shouldLoadConfigFromFile } from 'utils/build/file-configs';
import {
    Contracts,
    initContracts,
    setAggregatedBlockExecuteDeadline,
    setAggregatedBlockProveDeadline,
    setBlockCommitDeadlineMs,
    setEthSenderSenderAggregatedBlockCommitDeadline
} from './utils';
import path from 'path';
import { CONTRACT_DEPLOYER, CONTRACT_DEPLOYER_ADDRESS, hashBytecode, ZKSYNC_MAIN_ABI } from 'zksync-ethers/build/utils';
import { utils as zksync_utils } from 'zksync-ethers';
import { logsTestPath } from 'utils/build/logs';

async function logsPath(name: string): Promise<string> {
    return await logsTestPath(fileConfig.chain, 'logs/upgrade/', name);
}

const L2_BRIDGEHUB_ADDRESS = '0x0000000000000000000000000000000000010002';
const pathToHome = path.join(__dirname, '../../../..');
const fileConfig = shouldLoadConfigFromFile();

const contracts: Contracts = initContracts(pathToHome, fileConfig.loadFromFile);

const ZK_CHAIN_INTERFACE = JSON.parse(
    readFileSync(pathToHome + '/contracts/l1-contracts/out/IZKChain.sol/IZKChain.json').toString()
).abi;

const depositAmount = ethers.parseEther('0.001');

interface GatewayInfo {
    gatewayChainId: string;
    gatewayProvider: zksync.Provider;
    gatewayCTM: string;
    l2ChainAdmin: string;
    l2DiamondProxyAddress: string;
}

interface Call {
    target: string;
    value: BigNumberish;
    data: BytesLike;
}

describe('Upgrade test', function () {
    // Utility wallets for facilitating testing
    let tester: Tester;
    let alice: zksync.Wallet;

    // The wallet that controls the ecosystem governance on L1.
    let ecosystemGovWallet: ethers.Wallet;
    // Note, that if a chain is settling on Gateway, it is chain admin is aliased admin from L1.
    let l1AdminGovWallet: ethers.Wallet;

    // The address of the ecosystem governance. It is present on L1.
    let ecosystemGovernance: string;

    // The chain admin contract on the settlement layer.
    let l1ChainAdminContract: ethers.Contract;
    // The diamond proxy contract on the settlement layer.
    let slMainContract: ethers.Contract;

    let bootloaderHash: string;
    let defaultAccountHash: string;
    let evmEmulatorHash: string;
    let bytecodeSupplier: string;
    let executeOperation: string;
    let forceDeployAddress: string;
    let forceDeployBytecode: string;
    let logs: fs.FileHandle;

    let ethProviderAddress: string | undefined;
    let web3JsonRpc: string | undefined;
    let contractsL2DefaultUpgradeAddr: string;
    let deployerAddress: string;
    let complexUpgraderAddress: string;
    let upgradeAddress: string | undefined;
    let contractsPriorityTxMaxGasLimit: string;

    // Information about Gateway. Note, that it is non-null only if the chain settles
    // on top of Gateway, i.e. checking whether this variable is null allows to also know
    // whetheer the chain settles on top of Gateway.
    let gatewayInfo: GatewayInfo | null = null;

    let mainNodeSpawner: utils.NodeSpawner;

    before('Create test wallet', async () => {
        forceDeployAddress = '0xf04ce00000000000000000000000000000000000';
        deployerAddress = '0x0000000000000000000000000000000000008007';
        complexUpgraderAddress = '0x000000000000000000000000000000000000800f';
        logs = await fs.open(await logsPath('upgrade.log'), 'a');

        if (!fileConfig.loadFromFile) {
            throw new Error('Non file based not supported');
        }

        const generalConfig = loadConfig({
            pathToHome,
            chain: fileConfig.chain,
            config: 'general.yaml'
        });
        const contractsConfig = loadConfig({
            pathToHome,
            chain: fileConfig.chain,
            config: 'contracts.yaml'
        });
        const secretsConfig = loadConfig({
            pathToHome,
            chain: fileConfig.chain,
            config: 'secrets.yaml'
        });
        ethProviderAddress = secretsConfig.l1.l1_rpc_url;
        web3JsonRpc = generalConfig.api.web3_json_rpc.http_url;
        contractsL2DefaultUpgradeAddr = contractsConfig.l2.default_l2_upgrader;
        bytecodeSupplier = contractsConfig.ecosystem_contracts.l1_bytecodes_supplier_addr;
        contractsPriorityTxMaxGasLimit = '72000000';

        gatewayInfo = getGatewayInfo(pathToHome, fileConfig.chain);

        mainNodeSpawner = new utils.NodeSpawner(pathToHome, logs, fileConfig, {
            enableConsensus: false,
            ethClientWeb3Url: ethProviderAddress!,
            apiWeb3JsonRpcHttpUrl: web3JsonRpc!,
            baseTokenAddress: contractsConfig.l1.base_token_addr
        });

        tester = await Tester.init(ethProviderAddress!, web3JsonRpc!);
        alice = tester.emptyWallet();

        const chainWalletConfig = loadConfig({
            pathToHome,
            chain: fileConfig.chain,
            config: 'wallets.yaml'
        });

        l1AdminGovWallet = new ethers.Wallet(chainWalletConfig.governor.private_key, alice._providerL1());
        const ecosystemWalletConfig = loadConfig({
            pathToHome,
            chain: fileConfig.chain,
            configsFolder: '../../configs/',
            config: 'wallets.yaml'
        });

        // Note, that the following check is needed to reduce flackiness. In case the
        // `ecosystemGovWallet` and `l1AdminGovWallet` refer to the same account, then
        // sending transactions from the same account while using different `Wallet` objects
        // could lead to flacky issues.
        if (chainWalletConfig.governor.private_key == ecosystemWalletConfig.governor.private_key) {
            ecosystemGovWallet = l1AdminGovWallet;
        } else {
            ecosystemGovWallet = new ethers.Wallet(ecosystemWalletConfig.governor.private_key, alice._providerL1());
        }

        forceDeployBytecode = contracts.counterBytecode;

        l1ChainAdminContract = new ethers.Contract(
            contractsConfig.l1.chain_admin_addr,
            contracts.chainAdminAbi,
            tester.syncWallet.providerL1
        );

        slMainContract = gatewayInfo
            ? new ethers.Contract(gatewayInfo.l2DiamondProxyAddress, ZKSYNC_MAIN_ABI, gatewayInfo.gatewayProvider)
            : new ethers.Contract(contractsConfig.l1.diamond_proxy_addr, ZKSYNC_MAIN_ABI, tester.syncWallet.providerL1);

        const l1CtmContract = new ethers.Contract(
            contractsConfig.ecosystem_contracts.state_transition_proxy_addr,
            contracts.chainTypeManager,
            tester.syncWallet.providerL1
        );
        ecosystemGovernance = await l1CtmContract.owner();

        upgradeAddress = await deployDefaultUpgradeImpl(
            ecosystemGovWallet,
            contractsConfig.ecosystem_contracts.bridgehub_proxy_addr
        );
    });

    step('Run server and execute some transactions', async () => {
        // Set small timeouts.
        process.env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_COMMIT_DEADLINE = '1';
        process.env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_PROVE_DEADLINE = '1';
        process.env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = '1';
        // Must be > 1s, because bootloader requires l1 batch timestamps to be incremental.
        process.env.CHAIN_STATE_KEEPER_BLOCK_COMMIT_DEADLINE_MS = '2000';

        if (fileConfig.loadFromFile) {
            setEthSenderSenderAggregatedBlockCommitDeadline(pathToHome, fileConfig, 1);
            setAggregatedBlockProveDeadline(pathToHome, fileConfig, 1);
            setAggregatedBlockExecuteDeadline(pathToHome, fileConfig, 1);
            setBlockCommitDeadlineMs(pathToHome, fileConfig, 2000);
        }
        await mainNodeSpawner.killAndSpawnMainNode();

        let blocksCommitted = await slMainContract.getTotalBatchesCommitted();

        const initialL1BatchNumber = await tester.web3Provider.getL1BatchNumber();

        const baseToken = await tester.syncWallet.provider.getBaseTokenContractAddress();

        if (!zksync.utils.isAddressEq(baseToken, zksync.utils.ETH_ADDRESS_IN_CONTRACTS)) {
            await (await tester.syncWallet.approveERC20(baseToken, ethers.MaxUint256)).wait();
            await mintToAddress(baseToken, tester.ethWallet, tester.syncWallet.address, depositAmount * 10n);
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
        let newBlocksCommitted = await slMainContract.getTotalBatchesCommitted();
        let tryCount = 0;
        while (blocksCommitted === newBlocksCommitted && tryCount < 30) {
            newBlocksCommitted = await slMainContract.getTotalBatchesCommitted();
            tryCount += 1;
            await utils.sleep(1);
        }
    });

    step('Publish bytecodes', async () => {
        const bootloaderCode = readCode(
            // Different versions of foundry have different versions of the artifacts' paths
            [
                'contracts/system-contracts/zkout/playground_batch.yul/contracts-preprocessed/bootloader/playground_batch.yul.json',
                'contracts/system-contracts/zkout/playground_batch.yul/playground_batch.json',
                'contracts/system-contracts/zkout/playground_batch.yul/Bootloader.json'
            ],
            'contracts/system-contracts/bootloader/build/artifacts/playground_batch.yul.zbin'
        );

        const defaultAACode = readCode(
            ['contracts/system-contracts/zkout/DefaultAccount.sol/DefaultAccount.json'],
            'contracts/system-contracts/artifacts-zk/contracts-preprocessed/DefaultAccount.sol/DefaultAccount.json'
        );

        const evmEmulatorCode = readCode(
            // Different versions of foundry have different versions of the artifacts' paths
            [
                'contracts/system-contracts/zkout/EvmEmulator.yul/contracts-preprocessed/EvmEmulator.yul.json',
                'contracts/system-contracts/zkout/EvmEmulator.yul/EvmEmulator.json'
            ]
        );

        bootloaderHash = ethers.hexlify(zksync.utils.hashBytecode(bootloaderCode));
        defaultAccountHash = ethers.hexlify(zksync.utils.hashBytecode(defaultAACode));
        evmEmulatorHash = ethers.hexlify(zksync.utils.hashBytecode(evmEmulatorCode));

        let nonce = await tester.ethWallet.getNonce();
        nonce += await publishBytecode(tester.ethWallet, bytecodeSupplier, bootloaderCode, nonce);
        nonce += await publishBytecode(tester.ethWallet, bytecodeSupplier, defaultAACode, nonce);
        nonce += await publishBytecode(tester.ethWallet, bytecodeSupplier, evmEmulatorCode, nonce);
        await publishBytecode(tester.ethWallet, bytecodeSupplier, forceDeployBytecode, nonce);
    });

    step('Schedule governance call', async () => {
        const forceDeployment: ForceDeployment = {
            bytecodeHash: ethers.hexlify(zksync.utils.hashBytecode(forceDeployBytecode)),
            newAddress: forceDeployAddress,
            callConstructor: false,
            value: 0n,
            input: '0x'
        };

        const delegateCalldata = contracts.l2ForceDeployUpgraderAbi.encodeFunctionData('forceDeploy', [
            [forceDeployment]
        ]);
        const data = contracts.complexUpgraderAbi.encodeFunctionData('upgrade', [
            contractsL2DefaultUpgradeAddr,
            delegateCalldata
        ]);

        const { stmUpgradeData, chainUpgradeCalldata, setTimestampCalldata } = await prepareUpgradeCalldata(
            alice._providerL1(),
            alice._providerL2(),
            upgradeAddress!,
            {
                l2ProtocolUpgradeTx: {
                    txType: 254,
                    from: deployerAddress, // FORCE_DEPLOYER address
                    to: complexUpgraderAddress, // ComplexUpgrader address
                    gasLimit: contractsPriorityTxMaxGasLimit,
                    gasPerPubdataByteLimit: zksync.utils.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT,
                    maxFeePerGas: 0,
                    maxPriorityFeePerGas: 0,
                    paymaster: 0,
                    value: 0,
                    reserved: [0, 0, 0, 0],
                    data,
                    signature: '0x',
                    factoryDeps: [
                        bootloaderHash,
                        defaultAccountHash,
                        evmEmulatorHash,
                        ethers.hexlify(zksync.utils.hashBytecode(forceDeployBytecode))
                    ],
                    paymasterInput: '0x',
                    reservedDynamic: '0x'
                },
                bootloaderHash,
                upgradeTimestamp: 0
            },
            gatewayInfo
        );
        executeOperation = chainUpgradeCalldata;

        const pauseMigrationCalldata = await pauseMigrationsCalldata(
            alice._providerL1(),
            alice._providerL2(),
            gatewayInfo
        );
        console.log('Scheduling pause migration');
        await sendGovernanceOperation(pauseMigrationCalldata.scheduleTransparentOperation, 0, null);

        console.log('Sending pause migration');
        await sendGovernanceOperation(
            pauseMigrationCalldata.executeOperation,
            pauseMigrationCalldata.executeOperationValue,
            gatewayInfo ? gatewayInfo.gatewayProvider : null
        );

        console.log('Sending scheduleTransparentOperation');
        await sendGovernanceOperation(stmUpgradeData.scheduleTransparentOperation, 0, null);

        console.log('Sending executeOperation');
        await sendGovernanceOperation(
            stmUpgradeData.executeOperation,
            stmUpgradeData.executeOperationValue,
            gatewayInfo ? gatewayInfo.gatewayProvider : null
        );

        console.log('Sending chain admin operation');
        // ChainAdminOwnable.sol: `setUpgradeTimestamp` has onlyOwner so we call it directly.
        await chainAdminSetTimestamp(setTimestampCalldata);

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

        let lastBatchExecuted = await slMainContract.getTotalBatchesExecuted();
        let tryCount = 0;
        while (lastBatchExecuted < l1BatchNumber && tryCount < 40) {
            lastBatchExecuted = await slMainContract.getTotalBatchesExecuted();
            tryCount += 1;
            await utils.sleep(2);
        }
        if (lastBatchExecuted < l1BatchNumber) {
            throw new Error('Server did not execute old blocks');
        }

        // Execute the upgrade
        await sendChainAdminOperation(
            {
                target: await slMainContract.getAddress(),
                data: executeOperation,
                value: 0
            },
            gatewayInfo
        );

        let bootloaderHashL1 = await slMainContract.getL2BootloaderBytecodeHash();
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
        await mainNodeSpawner.killAndSpawnMainNode();

        // Trying to send a transaction from the same address again
        await checkedRandomTransfer(alice, 1n);
    });

    after('Try killing server', async () => {
        if (fileConfig.loadFromFile) {
            setEthSenderSenderAggregatedBlockCommitDeadline(pathToHome, fileConfig, 1);
            setAggregatedBlockProveDeadline(pathToHome, fileConfig, 10);
            setAggregatedBlockExecuteDeadline(pathToHome, fileConfig, 10);
            setBlockCommitDeadlineMs(pathToHome, fileConfig, 2500);
        }

        try {
            await utils.exec('pkill zksync_server');
        } catch (_) {}
    });

    async function sendGovernanceOperation(
        data: string,
        value: BigNumberish,
        providerForPriorityOp: zksync.Provider | null
    ) {
        const transaction = await ecosystemGovWallet.sendTransaction({
            to: ecosystemGovernance,
            value: value,
            data: data,
            type: 0
        });
        console.log(`Sent governance operation, tx_hash=${transaction.hash}, nonce=${transaction.nonce}`);
        const receipt = await transaction.wait();
        console.log(`Governance operation succeeded, tx_hash=${transaction.hash}`);

        // The governance operations may trigger additional L1->L2 transactions to gateway, which we should wait.
        if (providerForPriorityOp) {
            await waitForPriorityOps(receipt!, providerForPriorityOp);
        }
    }

    async function chainAdminSetTimestamp(data: string) {
        const transaction = await l1AdminGovWallet.sendTransaction({
            to: await l1ChainAdminContract.getAddress(),
            data,
            type: 0
        });
        console.log(`Sent chain admin operation, tx_hash=${transaction.hash}, nonce=${transaction.nonce}`);
        await transaction.wait();
        console.log(`Chain admin operation succeeded, tx_hash=${transaction.hash}`);
    }

    async function sendChainAdminOperation(call: Call, gatewayInfo: GatewayInfo | null) {
        if (gatewayInfo) {
            // We will have to perform an L1->L2 transaction to the gateway
            call = await composeL1ToL2Call(
                gatewayInfo.gatewayChainId,
                call.target,
                call.data,
                await (await alice.getBridgehubContract()).getAddress(),
                [],
                l1AdminGovWallet.provider!,
                // It does not matter who is the refund recipient in this test
                call.target
            );
        }

        const executeMulticallData = l1ChainAdminContract.interface.encodeFunctionData('multicall', [[call], true]);

        const transaction = await l1AdminGovWallet.sendTransaction({
            to: await l1ChainAdminContract.getAddress(),
            data: executeMulticallData,
            value: call.value,
            type: 0
        });
        console.log(`Sent chain admin operation, tx_hash=${transaction.hash}, nonce=${transaction.nonce}`);
        const receipt = await transaction.wait();
        console.log(`Chain admin operation succeeded, tx_hash=${transaction.hash}`);

        if (gatewayInfo) {
            await waitForPriorityOps(receipt!, gatewayInfo.gatewayProvider);
        }
    }

    // Note, that since Gateway is deployed with a whitelist, deploying new contracts is not allowed there by default.
    // To not bother with whitelisting in this test, we will perform an L1->L2 transaction via a governor (always whitelisted)
    // to deploy the default account.
    async function deployDefaultUpgradeImpl(governorWallet: ethers.Wallet, bridgehubAddr: string): Promise<string> {
        const bytecodePath = gatewayInfo
            ? pathToHome + '/contracts/l1-contracts/zkout/DefaultUpgrade.sol/DefaultUpgrade.json'
            : pathToHome + '/contracts/l1-contracts/out/DefaultUpgrade.sol/DefaultUpgrade.json';

        let bytecode = JSON.parse(readFileSync(bytecodePath).toString()).bytecode.object;
        if (!bytecode.startsWith('0x')) {
            bytecode = '0x' + bytecode;
        }

        if (gatewayInfo) {
            if (!governorWallet.provider) {
                throw new Error('Governor wallet does not have provider');
            }

            const salt = ethers.ZeroHash;
            const bytecodeHash = hashBytecode(bytecode);

            const governanceData = await prepareGovernanceCalldata(
                CONTRACT_DEPLOYER_ADDRESS,
                CONTRACT_DEPLOYER.encodeFunctionData('create2', [salt, bytecodeHash, '0x']),
                bridgehubAddr,
                governorWallet.provider,
                [bytecode],
                gatewayInfo
            );

            console.log('Scheduling deployment of the default upgrade');
            await sendGovernanceOperation(governanceData.scheduleTransparentOperation, 0, null);

            console.log('Executing deployment of the default upgrade');
            await sendGovernanceOperation(
                governanceData.executeOperation,
                governanceData.executeOperationValue,
                gatewayInfo.gatewayProvider
            );

            return zksync_utils.create2Address(
                zksync_utils.applyL1ToL2Alias(ecosystemGovernance),
                bytecodeHash,
                salt,
                '0x'
            );
        } else {
            const factory = new ethers.ContractFactory([], bytecode, governorWallet);
            return (await factory.deploy()).getAddress();
        }
    }
});

function readCode(newPaths: string[], legacyPath?: string): string {
    for (const newPath of newPaths) {
        let path = `${pathToHome}/${newPath}`;
        if (existsSync(path)) {
            return '0x'.concat(require(path).bytecode.object);
        }
    }

    if (legacyPath) {
        const path = `${pathToHome}/${legacyPath}`;
        if (path.endsWith('.zbin')) {
            return ethers.hexlify(readFileSync(path));
        } else {
            const legacyContent = require(path);
            return '0x'.concat(legacyContent.bytecode?.object || legacyContent.bytecode);
        }
    } else {
        throw new Error(`Cannot read contract at ${newPaths.join(',')}`);
    }
}

async function publishBytecode(
    wallet: ethers.Wallet,
    bytecodeSupplierAddr: string,
    bytecode: string,
    nonce: number
): Promise<number> {
    const hash = zksync.utils.hashBytecode(bytecode);
    const abi = [
        'function publishBytecode(bytes calldata _bytecode) public',
        'function publishingBlock(bytes32 _hash) public view returns (uint256)'
    ];

    const contract = new ethers.Contract(bytecodeSupplierAddr, abi, wallet);
    const block = await contract.publishingBlock(hash);
    if (block == BigInt(0)) {
        const tx = await contract.publishBytecode(bytecode, { nonce });
        await tx.wait();
        return 1;
    }
    return 0;
}

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
    l1Provider: ethers.Provider,
    l2Provider: zksync.Provider,
    upgradeAddress: string,
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
        bootloaderHash?: BytesLike;
        defaultAAHash?: BytesLike;
        evmEmulatorHash?: BytesLike;
        verifier?: string;
        verifierParams?: {
            recursionNodeLevelVkHash: BytesLike;
            recursionLeafLevelVkHash: BytesLike;
            recursionCircuitsSetVksHash: BytesLike;
        };
        l1ContractsUpgradeCalldata?: BytesLike;
        postUpgradeCalldata?: BytesLike;
        upgradeTimestamp: BigNumberish;
    },
    gatewayInfo: GatewayInfo | null
) {
    let settlementLayerDiamondProxy: ethers.Contract;
    if (gatewayInfo) {
        settlementLayerDiamondProxy = new ethers.Contract(
            gatewayInfo.l2DiamondProxyAddress,
            ZK_CHAIN_INTERFACE,
            gatewayInfo.gatewayProvider
        );
    } else {
        const zksyncAddress = await l2Provider.getMainContractAddress();
        settlementLayerDiamondProxy = new ethers.Contract(zksyncAddress, ZK_CHAIN_INTERFACE, l1Provider);
    }
    const settlementLayerCTMAddress = await settlementLayerDiamondProxy.getChainTypeManager();

    const oldProtocolVersion = Number(await settlementLayerDiamondProxy.getProtocolVersion());
    const newProtocolVersion = addToProtocolVersion(oldProtocolVersion, 1, 1);

    params.l2ProtocolUpgradeTx.nonce ??= BigInt(unpackNumberSemVer(newProtocolVersion)[1]);
    const upgradeInitData = contracts.l1DefaultUpgradeAbi.encodeFunctionData('upgrade', [
        [
            params.l2ProtocolUpgradeTx,
            params.bootloaderHash ?? ethers.ZeroHash,
            params.defaultAAHash ?? ethers.ZeroHash,
            params.evmEmulatorHash ?? ethers.ZeroHash,
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
    const stmUpgradeCalldata = contracts.chainTypeManager.encodeFunctionData('setNewVersionUpgrade', [
        upgradeParam,
        oldProtocolVersion,
        // The protocol version will not have any deadline in this upgrade
        ethers.MaxUint256,
        newProtocolVersion
    ]);

    // Execute this upgrade on a specific chain under this STM.
    const chainUpgradeCalldata = contracts.adminFacetAbi.encodeFunctionData('upgradeChainFromVersion', [
        oldProtocolVersion,
        upgradeParam
    ]);

    // Set timestamp for upgrade on a specific chain under this STM.
    const setTimestampCalldata = contracts.chainAdminAbi.encodeFunctionData('setUpgradeTimestamp', [
        newProtocolVersion,
        params.upgradeTimestamp
    ]);

    const bridgehubAddr = await l2Provider.getBridgehubContractAddress();

    const stmUpgradeData = await prepareGovernanceCalldata(
        settlementLayerCTMAddress,
        stmUpgradeCalldata,
        bridgehubAddr,
        l1Provider!,
        [],
        gatewayInfo
    );

    return {
        stmUpgradeData,
        chainUpgradeCalldata,
        setTimestampCalldata
    };
}

async function pauseMigrationsCalldata(
    l1Provider: ethers.Provider,
    l2Provider: zksync.Provider,
    gatewayInfo: GatewayInfo | null
) {
    const l1BridgehubAddr = await l2Provider.getBridgehubContractAddress();
    const to = gatewayInfo ? L2_BRIDGEHUB_ADDRESS : l1BridgehubAddr;

    const iface = new ethers.Interface(['function pauseMigration() external']);

    return prepareGovernanceCalldata(
        to,
        iface.encodeFunctionData('pauseMigration', []),
        l1BridgehubAddr,
        l1Provider,
        [],
        gatewayInfo
    );
}

interface UpgradeCalldata {
    scheduleTransparentOperation: string;
    executeOperation: string;
    executeOperationValue: BigNumberish;
}

async function prepareGovernanceCalldata(
    to: string,
    data: BytesLike,
    bridgehubAddr: string,
    l1Provider: ethers.Provider,
    factoryDeps: BytesLike[],
    gatewayInfo: GatewayInfo | null
): Promise<UpgradeCalldata> {
    let call;
    if (gatewayInfo) {
        // We will have to perform an L1->L2 transaction to the gateway
        call = await composeL1ToL2Call(
            gatewayInfo.gatewayChainId,
            to,
            data,
            bridgehubAddr,
            factoryDeps,
            l1Provider,
            // It does not matter who is the refund recipient in this test
            to
        );
    } else {
        call = {
            target: to,
            value: 0,
            data
        };
    }

    const governanceOperation = {
        calls: [call],
        predecessor: ethers.ZeroHash,
        // Use random salt for easier testing
        salt: ethers.randomBytes(32)
    };

    // Get transaction data of the `scheduleTransparent`
    const scheduleTransparentOperation = contracts.governanceAbi.encodeFunctionData('scheduleTransparent', [
        governanceOperation,
        0 // delay
    ]);

    // Get transaction data of the `execute`
    const executeOperation = contracts.governanceAbi.encodeFunctionData('execute', [governanceOperation]);

    return {
        scheduleTransparentOperation,
        executeOperation,
        executeOperationValue: call.value
    };
}

async function composeL1ToL2Call(
    chainId: string,
    to: string,
    data: BytesLike,
    bridgehubAddr: string,
    factoryDeps: BytesLike[],
    l1Provider: ethers.Provider,
    refundRecipient: string
): Promise<Call> {
    const gasPerPubdata = zksync.utils.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT;
    // Just a constant that needs to be large enough to handle upgrade-related things
    const gasLimit = 40_000_000;

    const gasPrice = (await l1Provider.getFeeData()).gasPrice! * BigInt(5);

    const bridgehub = new ethers.Contract(bridgehubAddr, zksync.utils.BRIDGEHUB_ABI, l1Provider);

    const baseCost = await bridgehub.l2TransactionBaseCost(chainId, gasPrice, gasLimit, gasPerPubdata);

    const encodedData = zksync.utils.BRIDGEHUB_ABI.encodeFunctionData('requestL2TransactionDirect', [
        {
            chainId: chainId,
            mintValue: baseCost,
            l2Contract: to,
            l2Value: 0,
            l2Calldata: data,
            l2GasLimit: gasLimit,
            l2GasPerPubdataByteLimit: gasPerPubdata,
            factoryDeps,
            refundRecipient
        }
    ]);

    return {
        target: bridgehubAddr,
        data: encodedData,
        // Works when ETH is the base token
        value: baseCost
    };
}

async function mintToAddress(
    baseTokenAddress: zksync.types.Address,
    ethersWallet: ethers.Wallet,
    addressToMintTo: string,
    amountToMint: bigint
) {
    const l1Erc20ABI = ['function mint(address to, uint256 amount)'];
    const l1Erc20Contract = new ethers.Contract(baseTokenAddress, l1Erc20ABI, ethersWallet);
    await (await l1Erc20Contract.mint(addressToMintTo, amountToMint)).wait();
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

export function getGatewayInfo(pathToHome: string, chain: string): GatewayInfo | null {
    const gatewayChainConfig = loadConfig({
        pathToHome,
        chain,
        config: 'gateway_chain.yaml'
    });
    if (!gatewayChainConfig) {
        return null;
    }

    if (gatewayChainConfig.gateway_chain_id) {
        const secretsConfig = loadConfig({
            pathToHome,
            chain,
            config: 'secrets.yaml'
        });

        return {
            gatewayChainId: gatewayChainConfig.gateway_chain_id,
            gatewayProvider: new zksync.Provider(secretsConfig.l1.gateway_rpc_url),
            gatewayCTM: gatewayChainConfig.state_transition_proxy_addr,
            l2ChainAdmin: gatewayChainConfig.chain_admin_addr,
            l2DiamondProxyAddress: gatewayChainConfig.diamond_proxy_addr
        };
    }

    return null;
}

async function waitForPriorityOps(receipt: ethers.TransactionReceipt, providerForPriorityOp: zksync.Provider) {
    const contract = await providerForPriorityOp.getMainContractAddress();
    const hash = zksync.utils.getL2HashFromPriorityOp(receipt, contract);
    console.log(`Gateway L1->L2 transaction with hash ${hash} detected`);

    await providerForPriorityOp.waitForTransaction(hash);
    console.log('Transaction complete!');
}

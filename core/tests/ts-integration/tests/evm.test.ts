import { TestMaster } from '../src';
import * as fs from 'fs';
import * as zksync from 'zksync-ethers';
import { ethers, BytesLike } from 'ethers';
import { scaledGasPrice, maxL2GasLimitForPriorityTxs } from '../src/helpers';
import { REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT } from 'zksync-ethers/build/utils';

interface ContractData {
    readonly abi: ethers.InterfaceAbi;
    readonly bytecode: { object: string };
}

enum ContractKind {
    TEST = 'test',
    SYSTEM = 'system'
}

function readContract(fileName: string, kind: ContractKind = ContractKind.TEST): ContractData {
    let basePath;
    switch (kind) {
        case ContractKind.TEST:
            basePath = './artifacts/evm-contracts';
            break;
        case ContractKind.SYSTEM:
            basePath = '../../../contracts/system-contracts/zkout';
            break;
    }
    return JSON.parse(fs.readFileSync(`${basePath}/${fileName}.sol/${fileName}.json`, { encoding: 'utf-8' }));
}

// Unless `RUN_EVM_TEST` is provided, skip the test suit
const describeEvm = process.env.RUN_EVM_TEST ? describe : describe.skip;

describeEvm('EVM contract checks', () => {
    let testMaster: TestMaster;
    let alice: ethers.Wallet;
    let counterFactory: ethers.ContractFactory;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = new ethers.Wallet(testMaster.mainAccount().privateKey, testMaster.mainAccount().provider);

        const contractDeployerData = readContract('ContractDeployer', ContractKind.SYSTEM);
        const contractDeployer = new ethers.Contract(
            '0x0000000000000000000000000000000000008006',
            contractDeployerData.abi,
            alice.provider
        );
        const allowedBytecodeTypes = await contractDeployer.allowedBytecodeTypesToDeploy();
        testMaster.reporter.debug('Allowed bytecode types', allowedBytecodeTypes);
        expect(allowedBytecodeTypes).toEqual(1n);

        const { abi, bytecode } = readContract('Counter');
        counterFactory = new ethers.ContractFactory(abi, bytecode, alice);
    });

    test('Should deploy counter wallet', async () => {
        const currentNonce = await alice.getNonce();
        const counter = await (await counterFactory.deploy(false)).waitForDeployment();
        const counterAddress = await counter.getAddress();
        const expectedAddress = ethers.getCreateAddress({ from: alice.address, nonce: currentNonce });
        expect(counterAddress).toEqual(expectedAddress);

        const newNonce = await alice.getNonce();
        expect(newNonce).toEqual(currentNonce + 1);
        const code = await alice.provider!!.getCode(counterAddress);
        expect(code.length).toBeGreaterThan(100);

        let currentValue = await counter.getFunction('get').staticCall();
        expect(currentValue).toEqual(0n);

        const incrementTx = await counter.getFunction('increment').populateTransaction(42n);
        await expect(await alice.sendTransaction(incrementTx)).toBeAccepted();

        currentValue = await counter.getFunction('get').staticCall();
        expect(currentValue).toEqual(42n);
    });

    test('Should not deploy when `to` is zero address', async () => {
        const deployTx = await counterFactory.getDeployTransaction(false);
        const bogusDeployTx = { to: '0x0000000000000000000000000000000000000000', ...deployTx };
        const currentNonce = await alice.getNonce();
        await expect(await alice.sendTransaction(bogusDeployTx)).toBeAccepted();
        const expectedAddress = ethers.getCreateAddress({ from: alice.address, nonce: currentNonce });
        const code = await alice.provider!!.getCode(expectedAddress);
        expect(code).toEqual('0x');
    });

    test('Should estimate gas for EVM transactions', async () => {
        const deployTx = await counterFactory.getDeployTransaction(false);
        const deploymentGas = await alice.estimateGas(deployTx);
        testMaster.reporter.debug('Deployment gas', deploymentGas);
        expect(deploymentGas).toBeGreaterThan(10_000n);

        const txReceipt = await (await alice.sendTransaction(deployTx)).wait();
        const counterAddress = txReceipt!!.contractAddress!!;
        const counterContract = new ethers.Contract(counterAddress, counterFactory.interface, alice.provider);
        const incrementTx = await counterContract.getFunction('increment').populateTransaction(42n);
        const incrementGas = await alice.estimateGas(incrementTx);
        testMaster.reporter.debug('Increment tx gas', incrementGas);
        expect(incrementGas).toBeGreaterThan(10_000n);
        expect(incrementGas).toBeLessThan(deploymentGas);
    });

    test('should propagate constructor revert', async () => {
        await expect(counterFactory.deploy(true)).rejects.toThrow('requested revert');
    });
});

describeEvm('createEVM/create2EVM functions checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let isETHBasedChain: boolean;
    let baseTokenAddress: string;
    let expectedL2Costs: bigint;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        baseTokenAddress = await alice._providerL2().getBaseTokenContractAddress();
        isETHBasedChain = baseTokenAddress == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;
        const gasPrice = await scaledGasPrice(alice);
        if (!isETHBasedChain) {
            expectedL2Costs =
                ((await alice.getBaseCost({
                    gasLimit: maxL2GasLimitForPriorityTxs(testMaster.environment().priorityTxMaxGasLimit),
                    gasPerPubdataByte: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT,
                    gasPrice
                })) *
                    140n) /
                100n;
        }
    });

    test('Should check that createEVM is callable and executes successfully', async () => {
        if (!isETHBasedChain) {
            const baseTokenDetails = testMaster.environment().baseToken;
            const baseTokenMaxAmount = await alice.getBalanceL1(baseTokenDetails.l1Address);
            await (await alice.approveERC20(baseTokenDetails.l1Address, baseTokenMaxAmount)).wait();
        }

        const contractInitcode: BytesLike = ethers.hexlify('0x69602a60005260206000f3600052600a6016f3');

        const abi = ['function createEVM(bytes _initCode)'];

        const iface = new ethers.Interface(abi);

        const ContractDeployerCalldata: BytesLike = iface.encodeFunctionData('createEVM', [contractInitcode]);

        const gasPrice = await scaledGasPrice(alice);
        const l2GasLimit = maxL2GasLimitForPriorityTxs(testMaster.environment().priorityTxMaxGasLimit);

        let priorityOpHandle = await alice.requestExecute({
            contractAddress: await zksync.utils.CONTRACT_DEPLOYER_ADDRESS,
            calldata: ContractDeployerCalldata,
            l2GasLimit: l2GasLimit,
            mintValue: isETHBasedChain ? 0n : expectedL2Costs,
            overrides: {
                gasPrice
            }
        });

        await priorityOpHandle.waitL1Commit();

        expect(priorityOpHandle).toBeAccepted();
    });

    test('Should check that create2EVM is callable and executes successfully', async () => {
        if (!isETHBasedChain) {
            const baseTokenDetails = testMaster.environment().baseToken;
            const baseTokenMaxAmount = await alice.getBalanceL1(baseTokenDetails.l1Address);
            await (await alice.approveERC20(baseTokenDetails.l1Address, baseTokenMaxAmount)).wait();
        }

        const contractInitcode: BytesLike = '0x69602a60005260206000f3600052600a6016f3';

        const abi = ['function create2EVM(bytes32 _salt, bytes _initCode)'];

        const iface = new ethers.Interface(abi);

        const ContractDeployerCalldata: BytesLike = iface.encodeFunctionData('create2EVM', [
            ethers.ZeroHash,
            contractInitcode
        ]);

        const gasPrice = await scaledGasPrice(alice);
        const l2GasLimit = maxL2GasLimitForPriorityTxs(testMaster.environment().priorityTxMaxGasLimit);

        let priorityOpHandle = await alice.requestExecute({
            contractAddress: await zksync.utils.CONTRACT_DEPLOYER_ADDRESS,
            calldata: ContractDeployerCalldata,
            l2GasLimit: l2GasLimit,
            mintValue: isETHBasedChain ? 0n : expectedL2Costs,
            overrides: {
                gasPrice
            }
        });

        await priorityOpHandle.waitL1Commit();

        expect(priorityOpHandle).toBeAccepted();
    });
});

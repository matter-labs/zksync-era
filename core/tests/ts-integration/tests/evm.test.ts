import { TestMaster } from '../src';
import * as fs from 'fs';
import { ethers } from 'ethers';

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

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

async function getAllowedBytecodeTypes(provider: ethers.Provider): Promise<bigint> {
    const contractDeployerData = readContract('ContractDeployer', ContractKind.SYSTEM);
    const contractDeployer = new ethers.Contract(
        '0x0000000000000000000000000000000000008006',
        contractDeployerData.abi,
        provider
    );
    return await contractDeployer.allowedBytecodeTypesToDeploy();
}

// Force EVM contract checks if `RUN_EVM_TEST` env var is set instead of auto-detecting EVM bytecode support. This is useful
// if we know for a fact that the tested chain should support EVM bytecodes (e.g., in CI).
const expectEvmSupport = Boolean(process.env.RUN_EVM_TEST);

describe('EVM contract checks', () => {
    let testMaster: TestMaster;
    let alice: ethers.Wallet;
    let counterFactory: ethers.ContractFactory;
    let allowedBytecodeTypes = 0n;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = new ethers.Wallet(testMaster.mainAccount().privateKey, testMaster.mainAccount().provider);

        allowedBytecodeTypes = await getAllowedBytecodeTypes(alice.provider!!);
        testMaster.reporter.debug('Allowed bytecode types', allowedBytecodeTypes);
        if (expectEvmSupport) {
            expect(allowedBytecodeTypes).toEqual(1n);
        }

        const { abi, bytecode } = readContract('Counter');
        counterFactory = new ethers.ContractFactory(abi, bytecode, alice);
    });

    // Wrappers for tests that expect EVM emulation to be enabled (`testEvm`) or disabled (`testNoEvm`).
    // Ideally, we'd want to use built-in `test.skip` functionality for these wrappers, but it's not possible to use it dynamically.
    const testEvm = (description: string, testFn: () => Promise<void>) => {
        test(description, async () => {
            if (allowedBytecodeTypes !== 1n) {
                testMaster.reporter.debug('EVM bytecodes are not supported; skipping');
                return;
            }
            await testFn();
        });
    };

    const testNoEvm = (description: string, testFn: () => Promise<void>) => {
        test(description, async () => {
            if (allowedBytecodeTypes === 1n) {
                testMaster.reporter.debug('EVM bytecodes are supported; skipping');
                return;
            }
            await testFn();
        });
    };

    testEvm('Should deploy counter wallet', async () => {
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

    testEvm('Should not deploy when `to` is zero address', async () => {
        const deployTx = await counterFactory.getDeployTransaction(false);
        const bogusDeployTx = { to: '0x0000000000000000000000000000000000000000', ...deployTx };
        const currentNonce = await alice.getNonce();
        await expect(await alice.sendTransaction(bogusDeployTx)).toBeAccepted();
        const expectedAddress = ethers.getCreateAddress({ from: alice.address, nonce: currentNonce });
        const code = await alice.provider!!.getCode(expectedAddress);
        expect(code).toEqual('0x');
    });

    testEvm('Should estimate gas for EVM transactions', async () => {
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

    testEvm('should propagate constructor revert', async () => {
        await expect(counterFactory.deploy(true)).rejects.toThrow('requested revert');
    });

    testNoEvm('should error on EVM contract deployment', async () => {
        await expect(counterFactory.deploy(false)).rejects.toThrow('toAddressIsNull');
    });
});

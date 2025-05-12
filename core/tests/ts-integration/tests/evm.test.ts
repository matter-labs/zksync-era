import { TestMaster } from '../src';
import * as fs from 'fs';
import { ethers } from 'ethers';

interface ContractData {
    readonly abi: ethers.InterfaceAbi;
    readonly bytecode: { object: string } | string;
    readonly deployedBytecode?: { object: string } | string;
}

function extractBytecode(bytecode: { object: string } | string) {
    return typeof bytecode === 'string' ? bytecode : bytecode.object;
}

enum ContractKind {
    TEST = 'test',
    SYSTEM = 'system'
}

function readContract(name: string, kind: ContractKind = ContractKind.TEST): ContractData {
    let fileName = name;
    let contractName = name;
    const splitPos = name.indexOf(':');
    if (splitPos >= 0) {
        fileName = name.substring(0, splitPos);
        contractName = name.substring(splitPos + 1);
    }

    let basePath;
    switch (kind) {
        case ContractKind.TEST:
            basePath = './artifacts/evm-contracts';
            break;
        case ContractKind.SYSTEM:
            basePath = '../../../contracts/system-contracts/zkout';
            break;
    }
    return JSON.parse(fs.readFileSync(`${basePath}/${fileName}.sol/${contractName}.json`, { encoding: 'utf-8' }));
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

enum ProviderKind {
    /** JSON-RPC provider from `zksync-ethers` */
    ZKSYNC = 'zksync',
    /** Vanilla JSON-RPC provider from `ethers` on L2 */
    ETHERS = 'ethers',
    /** `ethers` provider on L1 */
    L1_ETHERS = 'l1-ethers'
}

function describeEvm(providerKind: ProviderKind) {
    let testMaster: TestMaster;
    let alice: ethers.Wallet;
    let counterFactory: ethers.ContractFactory;
    let counterContractData: ContractData;
    let allowedBytecodeTypes = 0n;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        let provider: ethers.Provider;
        switch (providerKind) {
            case ProviderKind.ZKSYNC:
                provider = testMaster.mainAccount().provider;
                break;
            case ProviderKind.ETHERS:
                provider = testMaster.ethersProvider('L2');
                break;
            case ProviderKind.L1_ETHERS:
                provider = testMaster.ethersProvider('L1');
                break;
        }

        alice = new ethers.Wallet(testMaster.mainAccount().privateKey, provider);

        if (providerKind === ProviderKind.L1_ETHERS) {
            // Mark EVM bytecodes as enabled on L1.
            allowedBytecodeTypes = 1n;
        } else {
            allowedBytecodeTypes = await getAllowedBytecodeTypes(provider);
            testMaster.reporter.debug('Allowed bytecode types', allowedBytecodeTypes);
            if (expectEvmSupport) {
                expect(allowedBytecodeTypes).toEqual(1n);
            }
        }

        counterContractData = readContract('Counter');
        counterFactory = new ethers.ContractFactory(counterContractData.abi, counterContractData.bytecode, alice);
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
        const code = (await counter.getDeployedCode())!!;
        expect(code.length).toBeGreaterThan(100);
        const expectedBytecode = extractBytecode(counterContractData.deployedBytecode!!);
        expect(code).toEqual(expectedBytecode);

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

    testEvm('Should propagate constructor revert', async () => {
        await expect(counterFactory.deploy(true)).rejects.toThrow('requested revert');
    });

    testEvm('Should define contractAddress for failed deployments', async () => {
        const currentNonce = await alice.getNonce();
        const expectedAddress = ethers.getCreateAddress({ from: alice.address, nonce: currentNonce });
        const deployTx = await counterFactory.getDeployTransaction(true, { gasLimit: 10_000_000 });
        const deployTxResponse = await alice.sendTransaction(deployTx);
        try {
            await deployTxResponse.wait();
        } catch (e: any) {
            const receipt: ethers.TransactionReceipt = e.receipt;
            expect(receipt.status).toEqual(0);
            expect(receipt.contractAddress).toEqual(expectedAddress);
            return;
        }
        expect(null).fail('Expected deploy tx to be reverted');
    });

    testEvm('Should produce expected CREATE2 addresses', async () => {
        const { abi, bytecode } = readContract('Counter:CounterFactory');
        const create2Factory = await (
            await new ethers.ContractFactory(abi, bytecode, alice).deploy()
        ).waitForDeployment();
        const factoryAddress = await create2Factory.getAddress();

        const salt = ethers.getBytes('0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef');
        await expect(await create2Factory.getFunction('deploy').send(salt)).toBeAccepted();

        const deployArgs = counterFactory.interface.encodeDeploy([false]);
        const initCode = ethers.concat([counterFactory.bytecode, deployArgs]);
        const expectedAddress = ethers.getCreate2Address(factoryAddress, salt, ethers.keccak256(initCode));
        const counterAddress = await create2Factory.getFunction('getAddress').staticCall(salt);
        expect(counterAddress).toEqual(expectedAddress);

        const txCount = await alice.provider!!.getTransactionCount(factoryAddress);
        // The nonce is initially set to 1 on deployment, and then incremented on the CREATE2 call.
        expect(txCount).toEqual(2);

        // Check that the created contract can be called.
        const counter = new ethers.Contract(counterAddress, counterContractData.abi, alice);
        await expect(await counter.getFunction('increment').send(1n)).toBeAccepted();
        expect(await counter.getFunction('get').staticCall()).toEqual(1n);
    });

    testNoEvm('Should error on EVM contract deployment', async () => {
        // Vanilla `ethers` provider doesn't parse custom errors correctly.
        const errorMatcher = providerKind === ProviderKind.ZKSYNC ? 'toAddressIsNull' : undefined;
        await expect(counterFactory.deploy(false)).rejects.toThrow(errorMatcher);
    });
}

describe.each([ProviderKind.ZKSYNC, ProviderKind.ETHERS, ProviderKind.L1_ETHERS])(
    'EVM contract checks (provider: %s)',
    describeEvm
);

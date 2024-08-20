/**
 * Generic tests checking evm equivalence smart contract behavior.
 *
 * Note: if you are going to write multiple tests checking specific topic (e.g. `CREATE2` behavior or something like this),
 * consider creating a separate suite.
 * Let's try to keep only relatively simple and self-contained tests here.
 */

import { TestMaster } from '../src';
import { deployContract, getEVMArtifact, getEVMContractFactory, getTestContract } from '../src/helpers';

import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';

import fs, { PathLike } from 'fs';
import csv from 'csv-parser';
import { createObjectCsvWriter } from 'csv-writer';

const contracts = {
    tester: getTestContract('TestEVMCreate'),
    erc20: getTestContract('ERC20'),
    uniswapV2Pair: getTestContract('UniswapV2Pair'),
    uniswapV2Factory: getTestContract('UniswapV2Factory')
};

const artifacts = {
    counter: getEVMArtifact('../evm-contracts/CounterWithParam.sol'),
    proxyCaller: getEVMArtifact('../evm-contracts/ProxyCaller.sol'),
    creator: getEVMArtifact('../evm-contracts/Creator.sol'),
    erc20: getEVMArtifact('../evm-contracts/ERC20.sol'),
    constructorRevert: getEVMArtifact('../evm-contracts/ConstructorRevert.sol'),
    uniswapV2Pair: getEVMArtifact('../contracts/uniswap-v2/UniswapV2Factory.sol', 'UniswapV2Pair.sol'),
    uniswapV2Factory: getEVMArtifact('../contracts/uniswap-v2/UniswapV2Factory.sol', 'UniswapV2Factory.sol'),
    opcodeTest: getEVMArtifact('../evm-contracts/OpcodeTest.sol'),
    selfDestruct: getEVMArtifact('../evm-contracts/SelfDestruct.sol'),
    gasCaller: getEVMArtifact('../evm-contracts/GasCaller.sol'),
    counterFallback: getEVMArtifact('../evm-contracts/CounterFallback.sol'),
    uniswapFallback: getEVMArtifact('../evm-contracts/UniswapFallback.sol'),
    creatorFallback: getEVMArtifact('../evm-contracts/CreatorFallback.sol'),
    opcodeTestFallback: getEVMArtifact('../evm-contracts/OpcodeTestFallback.sol')
};

const initBytecode = '0x69602a60005260206000f3600052600a6016f3';
const runtimeBytecode = '0x602a60005260206000f3';

let gasLimit = '0x01ffffff';

const logGasCosts = false;
describe('EVM equivalence contract', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;

    // Contracts shared in several tests.
    let evmCreateTester: zksync.Contract;
    let deployer: zksync.Contract;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();

        evmCreateTester = await deployContract(alice, contracts.tester, []);
        deployer = new zksync.Contract(zksync.utils.CONTRACT_DEPLOYER_ADDRESS, zksync.utils.CONTRACT_DEPLOYER, alice);
    });

    describe('Gas consumption', () => {
        test("Should compare gas against counter fallback contract's call", async () => {
            const gasCallerContract = await deploygasCallerContract(alice, artifacts.gasCaller);

            const counterContract = await deploygasCallerContract(alice, artifacts.counterFallback);

            let result = (
                await gasCallerContract.getFunction('callAndGetGas').staticCall(counterContract.getAddress())
            ).toString();

            const expected_gas = '3617'; // Gas cost when run with solidity interpreter
            expect(result).toEqual(expected_gas);
        });

        test("Should compare gas against creator fallback contract's call", async () => {
            const gasCallerContract = await deploygasCallerContract(alice, artifacts.gasCaller);

            const creatorContract = await deploygasCallerContract(alice, artifacts.creatorFallback);

            let result = (
                await gasCallerContract.getFunction('callAndGetGas').staticCall(creatorContract.getAddress())
            ).toString();

            const expected_gas = '70598'; // Gas cost when run with solidity interpreter - 3 (We have some changes that are needed)
            expect(result).toEqual(expected_gas);
        });
    });

    describe('Contract creation', () => {
        describe('Create from EOA', () => {
            test('Should create evm contract from EOA and allow view and non-view calls', async () => {
                const args = 1;
                const factory = getEVMContractFactory(alice, artifacts.counter);
                const contract = await factory.deploy(args);
                await contract.deploymentTransaction()?.wait();
                const receipt = await alice.provider.getTransactionReceipt(
                    contract.deploymentTransaction()?.hash ??
                        (() => {
                            throw new Error('Deployment transaction has failed');
                        })()
                );

                await assertCreatedCorrectly(
                    deployer,
                    await contract.getAddress(),
                    '0x' + artifacts.counter.evm.deployedBytecode.object
                );

                expect((await contract.getFunction('get').staticCall()).toString()).toEqual('1');
                await (await contract.getFunction('increment')(1)).wait();
                expect((await contract.getFunction('get').staticCall()).toString()).toEqual('2');
            });

            test('Should create2 evm contract from ZKEVM contract', async () => {
                const salt = ethers.randomBytes(32);

                const expectedAddress = ethers.getCreate2Address(
                    await evmCreateTester.getAddress(),
                    salt,
                    ethers.keccak256(initBytecode)
                );

                const receipt = await (await evmCreateTester.create2(salt, initBytecode)).wait();

                await assertCreatedCorrectly(deployer, expectedAddress, runtimeBytecode);

                try {
                    await (await evmCreateTester.create2(salt, initBytecode, { gasLimit })).wait();
                } catch (e) {
                    // Should fail
                    return;
                }
                throw 'Should fail to create2 the same contract with same salt twice';
            });

            test('Should propegate revert in constructor', async () => {
                const factory = getEVMContractFactory(alice, artifacts.constructorRevert);
                const contract = await factory.deploy({ gasLimit });

                let failReason;

                try {
                    await contract.deploymentTransaction()?.wait();
                } catch (e: any) {
                    failReason = e.reason;
                }

                expect(failReason).toBe(null);
            });

            test('Should NOT create evm contract from EOA when `to` is address(0x0)', async () => {
                const args = 1;

                const factory = getEVMContractFactory(alice, artifacts.counter);
                const { data, ...rest } = await factory.getDeployTransaction(args);
                const dep_transaction = {
                    ...rest,
                    to: '0x0000000000000000000000000000000000000000',
                    chainId: alice.provider._network.chainId,
                    data
                };
                console.log(dep_transaction);
                const result = await (await alice.sendTransaction(dep_transaction)).wait();
                const expectedAddressCreate = ethers.getCreateAddress({
                    from: alice.address,
                    nonce: await alice.getNonce()
                });

                await assertContractNotCreated(deployer, expectedAddressCreate);
            });
        });
    });

    describe('Inter-contract calls', () => {
        test('Calls (read/write) between EVM contracts should work correctly', async () => {
            const args = 1;

            const counterFactory = getEVMContractFactory(alice, artifacts.counter);
            const counterContract = await counterFactory.deploy(args);
            await counterContract.deploymentTransaction()?.wait();
            await alice.provider.getTransactionReceipt(
                counterContract.deploymentTransaction()?.hash ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })()
            );

            const proxyCallerFactory = getEVMContractFactory(alice, artifacts.proxyCaller);
            const proxyCallerContract = await proxyCallerFactory.deploy();
            await proxyCallerContract.deploymentTransaction()?.wait();
            await alice.provider.getTransactionReceipt(
                proxyCallerContract.deploymentTransaction()?.hash ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })()
            );

            expect(
                (await proxyCallerContract.getFunction('proxyGet')(await counterContract.getAddress())).toString()
            ).toEqual('1');

            await (
                await proxyCallerContract.getFunction('executeIncrememt')(await counterContract.getAddress(), 1)
            ).wait();

            expect(
                (await proxyCallerContract.getFunction('proxyGet')(await counterContract.getAddress())).toString()
            ).toEqual('2');

            expect(
                (
                    await proxyCallerContract
                        .getFunction('proxyGetBytes')
                        .staticCall(await counterContract.getAddress())
                ).toString()
            ).toEqual('0x54657374696e67');
        });

        test('Create opcode works correctly', async () => {
            const creatorFactory = getEVMContractFactory(alice, artifacts.creator);
            const creatorContract = await creatorFactory.deploy();
            await creatorContract.deploymentTransaction()?.wait();

            const nonce = 1;

            const runtimeBytecode = await creatorContract.getFunction('getCreationRuntimeCode')();

            const expectedAddress = ethers.getCreateAddress({
                from: await creatorContract.getAddress(),
                nonce
            });

            const result = await (await creatorContract.getFunction('create')()).wait();

            await assertCreatedCorrectly(deployer, expectedAddress, runtimeBytecode);
        });

        test('Should revert correctly', async () => {
            const args = 1;

            const counterFactory = getEVMContractFactory(alice, artifacts.counter);
            const counterContract = await counterFactory.deploy(args);
            await counterContract.deploymentTransaction()?.wait();

            let errorString;

            try {
                await counterContract.getFunction('incrementWithRevert').staticCall(1, true);
            } catch (e: any) {
                errorString = e.reason;
            }

            expect(errorString).toEqual('This method always reverts');
        });
    });

    // NOTE: Gas cost comparisons should be done on a *fresh* chain that doesn't have e.g. bytecodes already published
    describe('ERC20', () => {
        let evmToken: ethers.BaseContract;
        let nativeToken: zksync.Contract;
        let userAccount: zksync.Wallet;
        let deployLogged: boolean = false;

        beforeEach(async () => {
            const erc20Factory = getEVMContractFactory(alice, artifacts.erc20);
            evmToken = await erc20Factory.deploy();
            await evmToken.deploymentTransaction()?.wait();
            nativeToken = await deployContract(alice, contracts.erc20, []);

            userAccount = testMaster.newEmptyAccount();
            // Only log the first deployment
            if (logGasCosts && !deployLogged) {
                let native_hash =
                    nativeToken.deploymentTransaction()?.hash ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })();
                let native_transanction_receipt =
                    (await alice.provider.getTransactionReceipt(native_hash)) ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })();
                let evm_hash =
                    evmToken.deploymentTransaction()?.hash ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })();
                let evm_transanction_receipt =
                    (await alice.provider.getTransactionReceipt(evm_hash)) ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })();
                console.log('ERC20 native deploy gas: ' + native_transanction_receipt.gasUsed);
                console.log('ERC20 evm deploy gas: ' + evm_transanction_receipt.gasUsed);
                deployLogged = true;
            }
            await (
                await alice.sendTransaction({
                    to: userAccount.address,
                    value: BigInt('0xffffffffffffff')
                })
            ).wait();
        });

        test('view functions should work', async () => {
            const evmBalanceOfCost = await evmToken.getFunction('balanceOf').estimateGas(alice.address);
            const nativeBalanceOfCost = await nativeToken.getFunction('balanceOf').estimateGas(alice.address);
            if (logGasCosts) {
                console.log('ERC20 native balanceOf gas: ' + nativeBalanceOfCost.toString());
                console.log('ERC20 evm balanceOf gas: ' + evmBalanceOfCost.toString());
            }
            expect((await evmToken.getFunction('balanceOf')(alice.address)).toString()).toEqual('1000000');
            expect((await evmToken.getFunction('totalSupply')()).toString()).toEqual('1000000');
            expect((await evmToken.getFunction('balanceOf')(userAccount.address)).toString()).toEqual('0');
        });

        test('transfer should work', async () => {
            expect((await evmToken.getFunction('balanceOf')(alice.address)).toString()).toEqual('1000000');
            const evmTransferTx = await (await evmToken.getFunction('transfer')(userAccount.address, 100000)).wait();
            const nativeTransferTx = await (await nativeToken.transfer(userAccount.address, 100000)).wait();
            if (logGasCosts) {
                console.log('ERC20 native transfer gas: ' + nativeTransferTx.gasUsed.toString());
                console.log('ERC20 evm transfer gas: ' + evmTransferTx.gasUsed.toString());
            }

            expect((await evmToken.getFunction('balanceOf')(alice.address)).toString()).toEqual('900000');
            expect((await evmToken.getFunction('balanceOf')(userAccount.address)).toString()).toEqual('100000');
        });

        test('approve & transferFrom should work', async () => {
            expect((await evmToken.getFunction('balanceOf')(alice.address)).toString()).toEqual('1000000');
            const evmApproveTx = await (
                await evmToken.connect(alice).getFunction('approve')(userAccount.getAddress(), 100000)
            ).wait();
            const nativeApproveTx = await (
                await nativeToken.connect(alice).getFunction('approve')(userAccount.address, 100000)
            ).wait();
            if (logGasCosts) {
                console.log('ERC20 native approve gas: ' + nativeApproveTx.gasUsed.toString());
                console.log('ERC20 evm approve gas: ' + evmApproveTx.gasUsed.toString());
            }

            const evmTransferFromTx = await (
                await evmToken.connect(userAccount).getFunction('transferFrom')(
                    alice.address,
                    userAccount.address,
                    100000
                )
            ).wait();
            const nativeTransferFromTx = await (
                await nativeToken.connect(userAccount).getFunction('transferFrom')(
                    alice.address,
                    userAccount.address,
                    100000
                )
            ).wait();
            if (logGasCosts) {
                console.log('ERC20 native transferFrom gas: ' + nativeTransferFromTx.gasUsed.toString());
                console.log('ERC20 evm transferFrom gas: ' + evmTransferFromTx.gasUsed.toString());
            }

            expect((await evmToken.getFunction('balanceOf')(alice.address)).toString()).toEqual('900000');
            expect((await evmToken.getFunction('balanceOf')(userAccount.address)).toString()).toEqual('100000');
        });
    });

    // NOTE: Gas cost comparisons should be done on a *fresh* chain that doesn't have e.g. bytecodes already published
    describe('Uniswap-v2', () => {
        let evmToken1: ethers.BaseContract;
        let evmToken2: ethers.BaseContract;
        let evmUniswapFactory: ethers.BaseContract;
        let nativeUniswapFactory: ethers.BaseContract;
        let evmUniswapPair: ethers.BaseContract;
        let nativeUniswapPair: ethers.BaseContract;

        let deployLogged: boolean = false;
        const NEW_PAIR_TOPIC = '0x0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9';

        beforeEach(async () => {
            const erc20Factory = getEVMContractFactory(alice, artifacts.erc20);
            evmToken1 = await erc20Factory.deploy({ gasLimit });
            await evmToken1.deploymentTransaction()?.wait();
            evmToken2 = await erc20Factory.deploy();
            await evmToken2.deploymentTransaction()?.wait();

            const evmUniswapFactoryFactory = getEVMContractFactory(alice, artifacts.uniswapV2Factory);
            evmUniswapFactory = await evmUniswapFactoryFactory.deploy('0x0000000000000000000000000000000000000000');
            await evmUniswapFactory.deploymentTransaction()?.wait();

            nativeUniswapFactory = await deployContract(
                alice,
                contracts.uniswapV2Factory,
                ['0x0000000000000000000000000000000000000000'],
                undefined,
                {
                    customData: {
                        factoryDeps: [contracts.uniswapV2Pair.bytecode]
                    }
                }
            );

            const evmPairReceipt = await (
                await evmUniswapFactory.getFunction('createPair')(evmToken1.getAddress(), evmToken2.getAddress())
            ).wait();

            const nativePairReceipt = await (
                await nativeUniswapFactory.getFunction('createPair')(evmToken1.getAddress(), evmToken2.getAddress())
            ).wait();

            const evmUniswapPairFactory = getEVMContractFactory(alice, artifacts.uniswapV2Pair);
            const nativeUniswapPairFactory = new zksync.ContractFactory(
                contracts.uniswapV2Pair.abi,
                contracts.uniswapV2Pair.bytecode,
                alice
            );
            evmUniswapPair = evmUniswapPairFactory.attach(
                ethers.AbiCoder.defaultAbiCoder().decode(
                    ['address', 'uint256'],
                    evmPairReceipt.logs.find((log: any) => log.topics[0] === NEW_PAIR_TOPIC).data
                )[0]
            );
            nativeUniswapPair = nativeUniswapPairFactory.attach(
                ethers.AbiCoder.defaultAbiCoder().decode(
                    ['address', 'uint256'],
                    nativePairReceipt.logs.find((log: any) => log.topics[0] === NEW_PAIR_TOPIC).data
                )[0]
            );
            const token1IsFirst = (await evmUniswapPair.getFunction('token0')()).toString() === evmToken1.getAddress();
            if (!token1IsFirst) {
                [evmToken1, evmToken2] = [evmToken2, evmToken1];
            }
            await (await evmToken1.getFunction('transfer')(evmUniswapPair.getAddress(), 100000)).wait();
            await (await evmToken1.getFunction('transfer')(nativeUniswapPair.getAddress(), 100000)).wait();
            await (await evmToken2.getFunction('transfer')(evmUniswapPair.getAddress(), 100000)).wait();
            await (await evmToken2.getFunction('transfer')(nativeUniswapPair.getAddress(), 100000)).wait();

            // Only log the first deployment
            if (logGasCosts && !deployLogged) {
                let native_hash =
                    nativeUniswapFactory.deploymentTransaction()?.hash ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })();
                let native_transanction_receipt =
                    (await alice.provider.getTransactionReceipt(native_hash)) ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })();
                let evm_hash =
                    evmUniswapFactory.deploymentTransaction()?.hash ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })();
                let evm_transanction_receipt =
                    (await alice.provider.getTransactionReceipt(evm_hash)) ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })();
                console.log('Uniswap Factory native deploy gas: ' + native_transanction_receipt.gasUsed);
                console.log('Uniswap Factory evm deploy gas: ' + evm_transanction_receipt.gasUsed);
                console.log('Uniswap Pair native create gas: ' + nativePairReceipt.gasUsed);
                console.log('Uniswap Pair evm create gas: ' + evmPairReceipt.gasUsed);
                deployLogged = true;
            }
        });

        test('mint, swap, and burn should work', async () => {
            const evmMintReceipt = await (await evmUniswapPair.getFunction('mint')(alice.address)).wait();
            const nativeMintReceipt = await (await nativeUniswapPair.getFunction('mint')(alice.address)).wait();

            await (await evmToken1.getFunction('transfer')(evmUniswapPair.getAddress(), 10000)).wait();
            await (await evmToken1.getFunction('transfer')(nativeUniswapPair.getAddress(), 10000)).wait();
            const evmSwapReceipt = await (
                await evmUniswapPair.getFunction('swap')(0, 5000, alice.address, '0x')
            ).wait();
            const nativeSwapReceipt = await (
                await nativeUniswapPair.getFunction('swap')(0, 5000, alice.address, '0x')
            ).wait();

            const evmLiquidityTransfer = await (
                await evmUniswapPair.getFunction('transfer')(
                    evmUniswapPair.getAddress(),
                    (await evmUniswapPair.getFunction('balanceOf')(alice.address)).toString()
                )
            ).wait();
            await (
                await nativeUniswapPair.getFunction('transfer')(
                    nativeUniswapPair.getAddress(),
                    (await nativeUniswapPair.getFunction('balanceOf')(alice.address)).toString()
                )
            ).wait();
            const evmBurnReceipt = await (await evmUniswapPair.getFunction('burn')(alice.address)).wait();
            const nativeBurnReceipt = await (await nativeUniswapPair.getFunction('burn')(alice.address)).wait();
            expect(Number((await evmToken1.getFunction('balanceOf')(alice.address)).toString())).toBeGreaterThanOrEqual(
                990000
            );
            expect(Number((await evmToken2.getFunction('balanceOf')(alice.address)).toString())).toBeGreaterThanOrEqual(
                990000
            );

            if (logGasCosts) {
                console.log('UniswapV2Pair native mint gas: ' + nativeMintReceipt.gasUsed);
                console.log('UniswapV2Pair evm mint gas: ' + evmMintReceipt.gasUsed);
                console.log('UniswapV2Pair native swap gas: ' + nativeSwapReceipt.gasUsed);
                console.log('UniswapV2Pair evm swap gas: ' + evmSwapReceipt.gasUsed);
                console.log('UniswapV2Pair native burn gas: ' + nativeBurnReceipt.gasUsed);
                console.log('UniswapV2Pair evm burn gas: ' + evmBurnReceipt.gasUsed);
            }
        });

        test("Should compare gas against uniswap fallback contract's call", async () => {
            const gasCallerFactory = getEVMContractFactory(alice, artifacts.gasCaller);
            const gasCallerContract = await gasCallerFactory.deploy();
            await gasCallerContract.deploymentTransaction()?.wait();
            await alice.provider.getTransactionReceipt(
                gasCallerContract.deploymentTransaction()?.hash ??
                    (() => {
                        throw new Error('Deployment transaction has failed');
                    })()
            );

            const uniswapContract = await deploygasCallerContract(alice, artifacts.uniswapFallback);
            await (await uniswapContract.getFunction('setUniswapAddress')(evmUniswapPair.getAddress())).wait();
            await (await uniswapContract.getFunction('setAliceAddress')(alice.address)).wait();

            await (await evmToken1.getFunction('transfer')(evmUniswapPair.getAddress(), 10000)).wait();
            await (await evmToken1.getFunction('transfer')(uniswapContract.getAddress(), 10000)).wait();

            let result = (
                await gasCallerContract.getFunction('callAndGetGas').staticCall(uniswapContract.getAddress())
            ).toString();

            const expected_gas = '165939'; // Gas cost when run with solidity interpreter
            expect(result).toEqual(expected_gas);
        });
    });

    // NOTE: Gas cost comparisons should be done on a *fresh* chain that doesn't have e.g. bytecodes already published
    // describe('Bulk opcode tests', () => {
    //     let opcodeTest: ethers.Contract;
    //     beforeEach(async () => {
    //         const opcodeTestFactory = getEVMContractFactory(alice, artifacts.opcodeTest);
    //         console.log(opcodeTestFactory.bytecode)
    //         opcodeTest = await opcodeTestFactory.deploy()
    //     });

    //     test('should successfully execute bulk opcode test', async () => {
    //         console.log(await deployer.evmCode(opcodeTest.address))
    //         // const receipt = await (await opcodeTest.execute()).wait()
    //     });
    // });

    afterAll(async () => {
        await testMaster.deinitialize();
        if (logGasCosts) {
            printCostData();
        }
    });
});

type BenchmarkResult = {
    name: string;
    used_zkevm_ergs: string;
    used_evm_gas: string;
    used_circuits: string;
};

async function saveBenchmark(name: string, filename: string, result: string) {
    try {
        const resultWithName = {
            name: name,
            used_zkevm_ergs: result,
            used_evm_gas: '0',
            used_circuits: '0'
        };

        let results: BenchmarkResult[] = [];

        // Read existing CSV file
        if (fs.existsSync(filename)) {
            const existingResults: BenchmarkResult[] = await new Promise((resolve, reject) => {
                const results: BenchmarkResult[] = [];
                fs.createReadStream(filename)
                    .pipe(csv())
                    .on('data', (data) => results.push(data))
                    .on('end', () => resolve(results))
                    .on('error', reject);
            });
            results = existingResults.map((result) => ({
                name: result.name,
                used_zkevm_ergs: result.used_zkevm_ergs,
                used_evm_gas: result.used_evm_gas,
                used_circuits: result.used_circuits
            }));
        }

        // Push the new result
        results.push(resultWithName);

        // Write results back to CSV
        const csvWriter = createObjectCsvWriter({
            path: filename,
            header: [
                { id: 'name', title: 'name' },
                { id: 'used_zkevm_ergs', title: 'used_zkevm_ergs' },
                { id: 'used_evm_gas', title: 'used_evm_gas' },
                { id: 'used_circuits', title: 'used_circuits' }
            ]
        });
        await csvWriter.writeRecords(results);

        console.log('Benchmark saved successfully.');
    } catch (error) {
        console.error('Error saving benchmark:', error);
    }
}
function zeroPad(num: number, places: number): string {
    return String(num).padStart(places, '0');
}

async function startBenchmark(): Promise<string> {
    try {
        const now = new Date();
        const year = now.getUTCFullYear();
        const month = zeroPad(now.getUTCMonth() + 1, 2); // Months are zero-based, so add 1
        const day = zeroPad(now.getUTCDate(), 2);
        const hour = zeroPad(now.getUTCHours(), 2);
        const minute = zeroPad(now.getUTCMinutes(), 2);
        const second = zeroPad(now.getUTCSeconds(), 2);
        const formattedTime = `${year}-${month}-${day}-${hour}-${minute}-${second}`;
        const directoryPath = 'benchmarks';

        if (!fs.existsSync(directoryPath)) {
            // If it doesn't exist, create it
            fs.mkdirSync(directoryPath);
        }

        const filename = `benchmarks/benchmark_integration_${formattedTime}.csv`;
        return filename;
    } catch (error) {
        console.error('Error creating benchmark:', error);
        return '';
    }
}

async function deploygasCallerContract(alice: zksync.Wallet, contract: any, ...args: Array<any>) {
    const counterFactory = getEVMContractFactory(alice, contract);
    const counterContract = await counterFactory.deploy(...args);
    await counterContract.waitForDeployment();
    await counterContract.deploymentTransaction()?.wait();
    let hash = counterContract.deploymentTransaction()?.hash;
    if (hash == undefined) {
        throw new Error('Deployment transaction has failed');
    }
    await alice.provider.getTransactionReceipt(hash);

    return counterContract;
}

async function assertStoredBytecodeHash(
    deployer: zksync.Contract,
    deployedAddress: string,
    expectedStoredHash: string
): Promise<void> {
    const ACCOUNT_CODE_STORAGE_ADDRESS = '0x0000000000000000000000000000000000008002';
    let runner =
        deployer.runner ??
        (() => {
            throw new Error('Runner get failed');
        })();
    let provider =
        runner.provider ??
        (() => {
            throw new Error('Provider get failed');
        })();
    const storedCodeHash = await provider.getStorage(
        ACCOUNT_CODE_STORAGE_ADDRESS,
        ethers.zeroPadValue(deployedAddress, 32)
    );

    expect(storedCodeHash).toEqual(expectedStoredHash);
}

async function assertCreatedCorrectly(
    deployer: zksync.Contract,
    deployedAddress: string,
    expectedEVMBytecode: string
): Promise<void> {
    const expectedStoredHash = getSha256BlobHash(expectedEVMBytecode);
    await assertStoredBytecodeHash(deployer, deployedAddress, expectedStoredHash);
}

function getPaddedBytecode(bytes: ethers.BytesLike) {
    const length = ethers.getBytes(bytes).length;

    const encodedLength = ethers.AbiCoder.defaultAbiCoder().encode(['uint256'], [length]);

    let paddedBytecode = encodedLength + ethers.toBeHex(ethers.toBigInt(bytes)).slice(2);

    // The length needs to be 32 mod 64. We use 64 mod 128, since
    // we are dealing with a hexlified string
    while ((paddedBytecode.length - 2) % 128 != 64) {
        paddedBytecode += '0';
    }

    return paddedBytecode;
}

// Returns the canonical code hash of
function getSha256BlobHash(bytes: ethers.BytesLike): string {
    const paddedBytes = getPaddedBytecode(bytes);

    const hash = ethers.getBytes(ethers.sha256(paddedBytes));
    hash[0] = 2;
    hash[1] = 0;

    // Length of the bytecode
    const lengthInBytes = ethers.getBytes(paddedBytes).length;
    hash[2] = Math.floor(lengthInBytes / 256);
    hash[3] = lengthInBytes % 256;

    return ethers.toBeHex(ethers.toBigInt(hash));
}

async function assertContractNotCreated(deployer: zksync.Contract, deployedAddress: string): Promise<void> {
    assertStoredBytecodeHash(deployer, deployedAddress, ethers.ZeroHash);
}

function printCostData() {
    let costsDataString = '';

    const averageOverhead =
        overheadDataDump.length === 0
            ? undefined
            : Math.floor(overheadDataDump.reduce((a: number, c: number) => a + c) / overheadDataDump.length);
    const minOverhead = overheadDataDump.length === 0 ? undefined : Math.min(...overheadDataDump);
    const maxOverhead = overheadDataDump.length === 0 ? undefined : Math.max(...overheadDataDump);

    costsDataString += 'Overhead\t' + averageOverhead + '\t' + minOverhead + '\t' + maxOverhead + '\n';

    Object.keys(opcodeDataDump).forEach((opcode) => {
        const opcodeString = '0x' + Number(opcode).toString(16).padStart(2, '0');
        const values = opcodeDataDump[opcode.toString()];
        if (values.length === 0) {
            costsDataString += opcodeString + '\n';
            return;
        }
        const average = Math.floor(values.reduce((a: number, c: number) => a + c) / values.length);
        const min = Math.min(...values);
        const max = Math.max(...values);

        costsDataString +=
            opcodeString +
            '\t' +
            average +
            '\t' +
            (min === average ? '' : min) +
            '\t' +
            (max === average ? '' : max) +
            '\n';
    });
    console.log(costsDataString);
}

const overheadDataDump: Array<number> = [];
const opcodeDataDump: any = {};
[
    '0x0',
    '0x1',
    '0x2',
    '0x3',
    '0x4',
    '0x5',
    '0x6',
    '0x7',
    '0x8',
    '0x9',
    '0x0A',
    '0x0B',
    '0x10',
    '0x11',
    '0x12',
    '0x13',
    '0x14',
    '0x15',
    '0x16',
    '0x17',
    '0x18',
    '0x19',
    '0x1A',
    '0x1B',
    '0x1C',
    '0x1D',
    '0x20',
    '0x30',
    '0x31',
    '0x32',
    '0x33',
    '0x34',
    '0x35',
    '0x36',
    '0x37',
    '0x38',
    '0x39',
    '0x3A',
    '0x3B',
    '0x3C',
    '0x3D',
    '0x3E',
    '0x3F',
    '0x40',
    '0x41',
    '0x42',
    '0x43',
    '0x44',
    '0x45',
    '0x46',
    '0x47',
    '0x48',
    '0x50',
    '0x51',
    '0x52',
    '0x53',
    '0x54',
    '0x55',
    '0x56',
    '0x57',
    '0x58',
    '0x59',
    '0x5A',
    '0x5B',
    '0x5F',
    '0x60',
    '0x61',
    '0x62',
    '0x63',
    '0x64',
    '0x65',
    '0x66',
    '0x67',
    '0x68',
    '0x69',
    '0x6A',
    '0x6B',
    '0x6C',
    '0x6D',
    '0x6E',
    '0x6F',
    '0x70',
    '0x71',
    '0x72',
    '0x73',
    '0x74',
    '0x75',
    '0x76',
    '0x77',
    '0x78',
    '0x79',
    '0x7A',
    '0x7B',
    '0x7C',
    '0x7D',
    '0x7E',
    '0x7F',
    '0x80',
    '0x81',
    '0x82',
    '0x83',
    '0x84',
    '0x85',
    '0x86',
    '0x87',
    '0x88',
    '0x89',
    '0x8A',
    '0x8B',
    '0x8C',
    '0x8D',
    '0x8E',
    '0x8F',
    '0x90',
    '0x91',
    '0x92',
    '0x93',
    '0x94',
    '0x95',
    '0x96',
    '0x97',
    '0x98',
    '0x99',
    '0x9A',
    '0x9B',
    '0x9C',
    '0x9D',
    '0x9E',
    '0x9F',
    '0xA0',
    '0xA1',
    '0xA2',
    '0xA3',
    '0xA4',
    '0xF0',
    '0xF1',
    '0xF2',
    '0xF3',
    '0xF4',
    '0xF5',
    '0xFA',
    '0xFD',
    '0xFE',
    '0xFF'
].forEach((key) => {
    opcodeDataDump[Number(key).toString()] = [];
});

async function dumpOpcodeLogs(hash: string, provider: zksync.Provider): Promise<void> {
    let tx_receipt =
        (await provider.getTransactionReceipt(hash)) ??
        (() => {
            throw new Error('Get Transaction Receipt has failed');
        })();
    const logs = tx_receipt.logs;
    logs.forEach((log) => {
        if (log.topics[0] === '0x63307236653da06aaa7e128a306b128c594b4cf3b938ef212975ed10dad17515') {
            //Overhead
            overheadDataDump.push(Number(ethers.AbiCoder.defaultAbiCoder().decode(['uint256'], log.data).toString()));
        } else if (log.topics[0] === '0xca5a69edf1b934943a56c00605317596b03e2f61c3f633e8657b150f102a3dfa') {
            // Opcode
            const parsed = ethers.AbiCoder.defaultAbiCoder().decode(['uint256', 'uint256'], log.data);
            const opcode = Number(parsed[0].toString());
            const cost = Number(parsed[1].toString());

            opcodeDataDump[opcode.toString()].push(cost);
        }
    });
}

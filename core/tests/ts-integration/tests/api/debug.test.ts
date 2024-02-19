/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../../src';
import { Token } from '../../src/types';

import * as zksync from 'zksync-web3';
import { ethers } from 'ethers';
import { BOOTLOADER_FORMAL_ADDRESS } from 'zksync-web3/build/src/utils';
import fs from 'fs';

describe('Debug methods', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let tokenDetails: Token;
    let aliceErc20: zksync.Contract;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();

        tokenDetails = testMaster.environment().erc20Token;
        aliceErc20 = new zksync.Contract(tokenDetails.l2Address, zksync.utils.IERC20, alice);
    });

    test('Should not fail for infinity recursion', async () => {
        const bytecodePath = `${process.env.ZKSYNC_HOME}/core/tests/ts-integration/contracts/zkasm/artifacts/deep_stak.zkasm/deep_stak.zkasm.zbin`;
        const bytecode = fs.readFileSync(bytecodePath);

        const contractFactory = new zksync.ContractFactory([], bytecode, testMaster.mainAccount());
        const deployTx = await contractFactory.deploy();
        const contractAddress = (await deployTx.deployed()).address;
        let txCallTrace = await testMaster.mainAccount().provider.send('debug_traceCall', [
            {
                to: contractAddress,
                data: '0x'
            }
        ]);
        let expected = {
            error: null,
            from: ethers.constants.AddressZero,
            gas: expect.any(String),
            gasUsed: expect.any(String),
            input: expect.any(String),
            output: '0x',
            revertReason: 'Error function_selector = 0x, data = 0x',
            to: BOOTLOADER_FORMAL_ADDRESS,
            type: 'Call',
            value: expect.any(String),
            calls: expect.any(Array)
        };
        expect(txCallTrace).toEqual(expected);
    });

    test('Debug sending erc20 token in a block', async () => {
        const value = ethers.BigNumber.from(200);
        await aliceErc20.transfer(bob.address, value).then((tx: any) => tx.wait());
        let tx = await aliceErc20.transfer(bob.address, value);
        let receipt = await tx.wait();
        let blockCallTrace = await testMaster
            .mainAccount()
            .provider.send('debug_traceBlockByNumber', [receipt.blockNumber.toString(16)]);
        let blockCallTrace_tracer = await testMaster
            .mainAccount()
            .provider.send('debug_traceBlockByNumber', [receipt.blockNumber.toString(16), { tracer: 'callTracer' }]);
        let expected = {
            error: null,
            from: ethers.constants.AddressZero,
            gas: expect.any(String),
            gasUsed: expect.any(String),
            input: expect.any(String),
            output: '0x',
            revertReason: null,
            to: BOOTLOADER_FORMAL_ADDRESS,
            type: 'Call',
            value: expect.any(String),
            calls: expect.any(Array)
        };
        for (let i = 0; i < blockCallTrace.length; i++) {
            expect(blockCallTrace[i]).toEqual({ result: expected });
            expect(blockCallTrace[i]).toEqual(blockCallTrace_tracer[i]);
        }
        expected = {
            error: null,
            from: ethers.constants.AddressZero,
            gas: expect.any(String),
            gasUsed: expect.any(String),
            input: `0xa9059cbb000000000000000000000000${bob.address
                .slice(2, 42)
                .toLowerCase()}00000000000000000000000000000000000000000000000000000000000000${value
                .toHexString()
                .slice(2, 4)}`,
            output: '0x',
            revertReason: null,
            to: BOOTLOADER_FORMAL_ADDRESS,
            type: 'Call',
            value: '0x0',
            calls: expect.any(Array)
        };
        let txCallTrace = await testMaster.mainAccount().provider.send('debug_traceTransaction', [tx.hash]);
        let txCallTrace_tracer = await testMaster
            .mainAccount()
            .provider.send('debug_traceTransaction', [tx.hash, { tracer: 'callTracer' }]);
        expect(txCallTrace).toEqual(expected);
        expect(txCallTrace).toEqual(txCallTrace_tracer);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

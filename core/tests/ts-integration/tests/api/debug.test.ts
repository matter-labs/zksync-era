/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../../src';
import { Token } from '../../src/types';

import * as zksync from 'zksync-web3';
import { ethers } from 'ethers';
import { BOOTLOADER_FORMAL_ADDRESS } from 'zksync-web3/build/src/utils';

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

    test('Debug sending erc20 token in a block', async () => {
        const value = ethers.BigNumber.from(200);
        await aliceErc20.transfer(bob.address, value).then((tx: any) => tx.wait());
        let tx = await aliceErc20.transfer(bob.address, value);
        let receipt = await tx.wait();
        let blockCallTrace = await testMaster
            .mainAccount()
            .provider.send('debug_traceBlockByNumber', [receipt.blockNumber.toString(16)]);
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
        expect(txCallTrace).toEqual(expected);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

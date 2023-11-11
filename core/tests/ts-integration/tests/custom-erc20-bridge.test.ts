/**
 * This suite contains tests checking the behavior of custom bridges.
 */

import { TestMaster } from '../src/index';
import { Token } from '../src/types';
import { spawn as _spawn } from 'child_process';

import * as zksync from 'zksync-web3';
import * as ethers from 'ethers';
import { scaledGasPrice } from '../src/helpers';
import {
    L1ERC20BridgeFactory,
    TransparentUpgradeableProxyFactory,
    AllowListFactory
} from 'l1-zksync-contracts/typechain';
import { sleep } from 'zk/build/utils';

describe('Tests for the custom bridge behavior', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let tokenDetails: Token;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();
        tokenDetails = testMaster.environment().erc20Token;
    });

    test('Should deploy custom bridge', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        let balance = await alice.getBalanceL1();
        let transferTx = await alice._signerL1().sendTransaction({
            to: bob.address,
            value: balance.div(2)
        });
        await transferTx.wait();

        let allowList = new AllowListFactory(alice._signerL1());
        let allowListContract = await allowList.deploy(alice.address);
        await allowListContract.deployTransaction.wait(2);

        // load the l1bridge contract
        let l1bridgeFactory = new L1ERC20BridgeFactory(alice._signerL1());
        const gasPrice = await scaledGasPrice(alice);

        let l1Bridge = await l1bridgeFactory.deploy(
            process.env.CONTRACTS_DIAMOND_PROXY_ADDR!,
            allowListContract.address
        );
        await l1Bridge.deployTransaction.wait(2);
        let l1BridgeProxyFactory = new TransparentUpgradeableProxyFactory(alice._signerL1());
        let l1BridgeProxy = await l1BridgeProxyFactory.deploy(l1Bridge.address, bob.address, '0x');
        const amount = 1000; // 1000 wei is enough.
        await l1BridgeProxy.deployTransaction.wait(2);

        const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
        const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `cd $ZKSYNC_HOME && yarn l1-contracts`;
        let args = `--private-key ${alice.privateKey} --erc20-bridge ${l1BridgeProxy.address}`;
        let command = `${baseCommandL1} initialize-bridges ${args}`;
        await spawn(command);

        const setAccessModeTx = await allowListContract.setAccessMode(l1BridgeProxy.address, 2);
        await setAccessModeTx.wait();

        let l1bridge2 = new L1ERC20BridgeFactory(alice._signerL1()).attach(l1BridgeProxy.address);

        const maxAttempts = 5;
        let ready = false;
        for (let i = 0; i < maxAttempts; ++i) {
            const l2Bridge = await l1bridge2.l2Bridge();
            if (l2Bridge != ethers.constants.AddressZero) {
                const code = await alice._providerL2().getCode(l2Bridge);
                if (code.length > 0) {
                    ready = true;
                    break;
                }
            }
            await sleep(1);
        }
        if (!ready) {
            throw new Error('Failed to wait for the l2 bridge init');
        }

        let l2TokenAddress = await l1bridge2.callStatic.l2TokenAddress(tokenDetails.l1Address);
        const initialBalanceL1 = await alice.getBalanceL1(tokenDetails.l1Address);
        const initialBalanceL2 = await alice.getBalance(l2TokenAddress);
        let tx = await alice.deposit({
            token: tokenDetails.l1Address,
            amount,
            approveERC20: true,
            approveOverrides: {
                gasPrice
            },
            overrides: {
                gasPrice
            },
            bridgeAddress: l1BridgeProxy.address
        });

        await tx.wait();
        await expect(alice.getBalanceL1(tokenDetails.l1Address)).resolves.bnToBeEq(initialBalanceL1.sub(amount));
        await expect(alice.getBalance(l2TokenAddress)).resolves.bnToBeEq(initialBalanceL2.add(amount));
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

// executes a command in a new shell
// but pipes data to parent's stdout/stderr
export function spawn(command: string) {
    command = command.replace(/\n/g, ' ');
    const child = _spawn(command, { stdio: 'inherit', shell: true });
    return new Promise((resolve, reject) => {
        child.on('error', reject);
        child.on('close', (code) => {
            code == 0 ? resolve(code) : reject(`Child process exited with code ${code}`);
        });
    });
}

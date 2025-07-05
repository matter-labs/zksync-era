/**
 * This suite contains tests for the Web3 API compatibility and ZKsync-specific extensions.
 */
import { TestMaster } from '../../src';
import * as zksync from 'zksync-ethers';
import { types } from 'zksync-ethers';
import * as ethers from 'ethers';
import { anyTransaction, deployContract, getTestContract } from '../../src/helpers';
import { shouldOnlyTakeFee } from '../../src/modifiers/balance-checker';
import fetch, { RequestInit } from 'node-fetch';
import { EIP712_TX_TYPE, PRIORITY_OPERATION_L2_TX_TYPE } from 'zksync-ethers/build/utils';
import { NodeMode } from '../../src/types';
import { waitForNewL1Batch } from 'utils';

// Regular expression to match variable-length hex number.
const HEX_VALUE_REGEX = /^0x[\da-fA-F]*$/;
const DATE_REGEX = /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{6})?/;

const contracts = {
    counter: getTestContract('Counter'),
    events: getTestContract('Emitter'),
    outer: getTestContract('Outer'),
    inner: getTestContract('Inner'),
    stateOverride: getTestContract('StateOverrideTest')
};

describe('web3 API compatibility tests', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let l2Token: string;
    let chainId: bigint;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        l2Token = testMaster.environment().erc20Token.l2Address;
        chainId = testMaster.environment().l2ChainId;
    });

    test('Should test block/transaction web3 methods', async () => {
        const blockNumber = 1;
        const blockNumberHex = '0x1';

        // eth_getBlockByNumber
        const blockHash = (await alice.provider.getBlock(blockNumber)).hash!;
        const blockWithTxsByNumber = await alice.provider.getBlock(blockNumber, true);
        expect(blockWithTxsByNumber.gasLimit).toBeGreaterThan(0n);

        // `ethers.Block` doesn't include `logsBloom` for some reason.
        const blockByNumberFull = await alice.provider.send('eth_getBlockByNumber', [blockNumberHex, false]);
        expect(blockByNumberFull.logsBloom).toEqual(expect.stringMatching(HEX_VALUE_REGEX));
        expect(blockByNumberFull.logsBloom.length).toEqual(514);
        expect(blockByNumberFull.logsBloom != ethers.zeroPadValue('0x00', 256)).toBeTruthy();

        let sumTxGasUsed = 0n;
        for (const tx of blockWithTxsByNumber.prefetchedTransactions) {
            const receipt = await alice.provider.getTransactionReceipt(tx.hash);
            sumTxGasUsed = sumTxGasUsed + receipt!.gasUsed;
        }
        expect(blockWithTxsByNumber.gasUsed).toBeGreaterThanOrEqual(sumTxGasUsed);

        let expectedReceipts = [];
        let expectedBloom = blockByNumberFull.logsBloom.toLowerCase();

        let blockBloomFromReceipts = new Uint8Array(256);
        for (const tx of blockWithTxsByNumber.prefetchedTransactions) {
            const receipt = await alice.provider.send('eth_getTransactionReceipt', [tx.hash]);
            expectedReceipts.push(receipt);

            let receiptBloom = ethers.getBytes(receipt.logsBloom);
            for (let i = 0; i < blockBloomFromReceipts.length; i++) {
                blockBloomFromReceipts[i] = blockBloomFromReceipts[i] | receiptBloom[i];
            }
        }

        expect(ethers.hexlify(blockBloomFromReceipts)).toEqual(expectedBloom);

        let receipts = await alice.provider.send('eth_getBlockReceipts', [blockNumberHex]);
        expect(receipts).toEqual(expectedReceipts);

        // eth_getBlockByHash
        await alice.provider.getBlock(blockHash);
        const blockWithTxsByHash = await alice.provider.getBlock(blockHash, true);
        expect(blockWithTxsByNumber.number).toEqual(blockWithTxsByHash.number);

        // eth_getBlockTransactionCountByNumber
        const txCountByNumber = await alice.provider.send('eth_getBlockTransactionCountByNumber', [blockNumberHex]);
        expect(parseInt(txCountByNumber, 16)).toEqual(blockWithTxsByNumber.prefetchedTransactions.length);

        // eth_getBlockTransactionCountByHash
        const txCountByHash = await alice.provider.send('eth_getBlockTransactionCountByHash', [blockHash]);
        expect(parseInt(txCountByHash, 16)).toEqual(blockWithTxsByNumber.prefetchedTransactions.length);

        // eth_getTransactionByBlockNumberAndIndex
        const txByBlockNumberAndIndex = await alice.provider.send('eth_getTransactionByBlockNumberAndIndex', [
            blockNumberHex,
            '0x0'
        ]);

        // eth_getTransactionByBlockHashAndIndex
        const txByBlockHashAndIndex = await alice.provider.send('eth_getTransactionByBlockHashAndIndex', [
            blockHash,
            '0x0'
        ]);
        expect(txByBlockHashAndIndex.hash).toEqual(txByBlockNumberAndIndex.hash);

        // eth_getTransactionByHash
        const txByHash = await alice.provider.send('eth_getTransactionByHash', [txByBlockNumberAndIndex.hash]);
        expect(txByHash.hash).toEqual(txByBlockNumberAndIndex.hash);
    });

    test('Should test storage web3 methods', async () => {
        const counterContract = await deployContract(alice, contracts.counter, []);

        // eth_getCode
        const code = await alice.provider.getCode(await counterContract.getAddress());
        expect(code).toEqual(ethers.hexlify(contracts.counter.bytecode));

        // eth_getStorageAt
        const accCodeStorageAddress = '0x0000000000000000000000000000000000008002';
        const codeKey = '0x000000000000000000000000' + (await counterContract.getAddress()).substring(2);
        const codeHash = await alice.provider.getStorage(accCodeStorageAddress, codeKey);

        const expectedHash = ethers.sha256(contracts.counter.bytecode);
        expect(codeHash.substring(10)).toEqual(expectedHash.substring(10));
    });

    test('Should test some zks web3 methods', async () => {
        // zks_L1ChainId
        const l1ChainId = (await alice.providerL1!.getNetwork()).chainId;
        const l1ChainIdFromL2Provider = BigInt(await alice.provider.l1ChainId());
        expect(l1ChainId).toEqual(l1ChainIdFromL2Provider);
        // zks_getBlockDetails
        const blockDetails = await alice.provider.getBlockDetails(1);
        const block = await alice.provider.getBlock(1);
        expect(blockDetails.rootHash).toEqual(block.hash);
        expect(blockDetails.l1BatchNumber).toEqual(block.l1BatchNumber);
        // zks_getL1BatchDetails
        const batchDetails = await alice.provider.getL1BatchDetails(block.l1BatchNumber!);
        expect(batchDetails.number).toEqual(block.l1BatchNumber);
        // zks_estimateFee
        const response = await alice.provider.send('zks_estimateFee', [
            { from: alice.address, to: alice.address, value: '0x1' }
        ]);
        const expectedResponse = {
            gas_limit: expect.stringMatching(HEX_VALUE_REGEX),
            gas_per_pubdata_limit: expect.stringMatching(HEX_VALUE_REGEX),
            max_fee_per_gas: expect.stringMatching(HEX_VALUE_REGEX),
            max_priority_fee_per_gas: expect.stringMatching(HEX_VALUE_REGEX)
        };
        expect(response).toMatchObject(expectedResponse);
    });

    test('Should check the network version', async () => {
        // Valid network IDs for ZKsync are greater than 270.
        // This test suite may run on different envs, so we don't expect a particular ID.
        await expect(alice.provider.send('net_version', [])).resolves.toMatch(chainId.toString());
    });

    test('Should check the syncing status', async () => {
        // We can't know whether the node is synced (in EN case), so we just check the validity of the response.
        const response = await alice.provider.send('eth_syncing', []);
        // Sync status is either `false` or an object with the following fields.
        if (response !== false) {
            const expectedObject = {
                currentBlock: expect.stringMatching(HEX_VALUE_REGEX),
                highestBlock: expect.stringMatching(HEX_VALUE_REGEX),
                startingBlock: expect.stringMatching(HEX_VALUE_REGEX)
            };
            expect(response).toMatchObject(expectedObject);
        }
    });

    // @ts-ignore
    test.each([
        ['net_peerCount', [], '0x0'],
        ['net_listening', [], false],
        ['web3_clientVersion', [], 'zkSync/v2.0'],
        ['eth_accounts', [], []],
        ['eth_coinbase', [], '0x0000000000000000000000000000000000000000'],
        ['eth_getCompilers', [], []],
        ['eth_hashrate', [], '0x0'],
        ['eth_mining', [], false],
        ['eth_getUncleCountByBlockNumber', ['0x0'], '0x0'],
        ['eth_maxPriorityFeePerGas', [], '0x0']
    ])('Should test bogus web3 methods (%s)', async (method: string, input: string[], output: any) => {
        await expect(alice.provider.send(method, input)).resolves.toEqual(output);
    });

    test('Should test bogus web3 methods (eth_getUncleCountByBlockHash)', async () => {
        // This test can't be represented as a part of the table, since the input is dynamic.
        const firstBlockHash = (await alice.provider.getBlock(1)).hash;
        await expect(alice.provider.send('eth_getUncleCountByBlockHash', [firstBlockHash])).resolves.toEqual('0x0');
    });

    test('Should test current protocol version', async () => {
        // Node should report well-formed semantic protocol version
        await expect(alice.provider.send('eth_protocolVersion', [])).resolves.toMatch(/^zks\/0\.\d+\.\d+$/);
    });

    test('Should test web3 response extensions', async () => {
        if (testMaster.isFastMode()) {
            // This test requires a new L1 batch to be created, which may be very time-consuming on stage.
            return;
        }

        const amount = 1;
        const erc20ABI = ['function transfer(address to, uint256 amount)'];
        const erc20contract = new ethers.Contract(l2Token, erc20ABI, alice);
        const tx = await erc20contract.transfer(alice.address, amount).then((tx: any) => tx.wait());

        // Trigger new L1 batch for all the fields in the receipt to be present.
        // Normally it's faster than to wait for tx finalization.
        await waitForNewL1Batch(alice);

        // We must get the receipt explicitly, because the receipt obtained via `tx.wait()` could resolve
        // *before* the batch was created and not have all the fields set.
        const receipt = await alice.provider.getTransactionReceipt(tx.hash);
        const logs = await alice.provider.getLogs({
            fromBlock: receipt!.blockNumber,
            toBlock: receipt!.blockNumber
        });
        const block = await alice.provider.getBlock(receipt!.blockNumber);
        const blockWithTransactions = await alice.provider.getBlock(receipt!.blockNumber, true);
        const tx1 = await alice.provider.getTransaction(tx.hash);
        expect(tx1.l1BatchNumber).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
        expect(tx1.l1BatchTxIndex).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
        expect(tx1.chainId).toEqual(chainId);
        expect(tx1.type).toEqual(EIP712_TX_TYPE);

        const EIP1559_TX_TYPE = 2;
        expect(receipt!.l1BatchNumber).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
        expect(receipt!.l1BatchTxIndex).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
        expect(receipt!.logs[0].l1BatchNumber).toEqual(receipt!.l1BatchNumber);
        expect(logs[0].l1BatchNumber).toEqual(receipt!.l1BatchNumber);
        expect(block.l1BatchNumber).toEqual(receipt!.l1BatchNumber);
        expect(block.l1BatchTimestamp).toEqual(expect.anything());
        expect(blockWithTransactions.l1BatchNumber).toEqual(receipt!.l1BatchNumber);
        expect(blockWithTransactions.l1BatchTimestamp).toEqual(expect.anything());

        for (const tx of blockWithTransactions.prefetchedTransactions) {
            expect(tx.l1BatchNumber).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
            expect(tx.l1BatchTxIndex).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
            expect(tx.chainId).toEqual(chainId);
            expect([0, EIP712_TX_TYPE, PRIORITY_OPERATION_L2_TX_TYPE, EIP1559_TX_TYPE]).toContain(tx.type);
        }
    });

    test('Should check transactions from API / Legacy tx', async () => {
        const LEGACY_TX_TYPE = 0;
        const gasPrice = (await alice._providerL2().getGasPrice()) * 2n;
        const legacyTx = await alice.sendTransaction({
            type: LEGACY_TX_TYPE,
            to: alice.address,
            gasPrice
        });
        await legacyTx.wait();

        const legacyApiReceipt = await alice.provider.getTransaction(legacyTx.hash);
        expect(legacyApiReceipt.gasPrice).toEqual(gasPrice);
    });

    test('Should check transactions from API / EIP1559 tx', async () => {
        const EIP1559_TX_TYPE = 2;
        const eip1559Tx = await alice.sendTransaction({
            type: EIP1559_TX_TYPE,
            to: alice.address
        });
        await eip1559Tx.wait();

        const eip1559ApiReceipt = await alice.provider.getTransaction(eip1559Tx.hash);
        expect(eip1559ApiReceipt.maxFeePerGas).toEqual(eip1559Tx.maxFeePerGas!);
        // `ethers` will use value provided by `eth_maxPriorityFeePerGas`, and we return 0 there.
        expect(eip1559ApiReceipt.maxPriorityFeePerGas).toEqual(0n);
    });

    test('Should test getFilterChanges for pending transactions', async () => {
        if (testMaster.environment().nodeMode === NodeMode.External) {
            // Pending transactions logic doesn't work on EN since we don't have a proper mempool -
            // transactions only appear in the DB after they are included in the block.
            return;
        }

        // We will need to wait until the mempool cache on the server is updated.
        // The default update period is 50 ms, so we will wait for 75 ms to be sure.
        const MEMPOOL_CACHE_WAIT = 50 + 25;

        const filterId = await alice.provider.send('eth_newPendingTransactionFilter', []);
        let changes: string[] = await alice.provider.send('eth_getFilterChanges', [filterId]);

        const tx1 = await alice.sendTransaction({
            to: alice.address
        });
        testMaster.reporter.debug(`Sent a transaction ${tx1.hash}`);

        while (!changes.includes(tx1.hash)) {
            await zksync.utils.sleep(MEMPOOL_CACHE_WAIT);
            changes = await alice.provider.send('eth_getFilterChanges', [filterId]);
            testMaster.reporter.debug('Received filter changes', changes);
        }
        expect(changes).toContain(tx1.hash);

        const tx2 = await alice.sendTransaction({
            to: alice.address
        });
        const tx3 = await alice.sendTransaction({
            to: alice.address
        });
        const tx4 = await alice.sendTransaction({
            to: alice.address
        });
        const remainingHashes = new Set([tx2.hash, tx3.hash, tx4.hash]);
        testMaster.reporter.debug('Sent new transactions with hashes', remainingHashes);

        while (remainingHashes.size > 0) {
            await zksync.utils.sleep(MEMPOOL_CACHE_WAIT);
            changes = await alice.provider.send('eth_getFilterChanges', [filterId]);
            testMaster.reporter.debug('Received filter changes', changes);

            expect(changes).not.toContain(tx1.hash);
            for (const receivedHash of changes) {
                remainingHashes.delete(receivedHash);
            }
        }
    });

    test('Should test pub-sub API: blocks', async () => {
        // Checks that we can receive an event for new block being created.
        let wsProvider = new ethers.WebSocketProvider(testMaster.environment().wsL2NodeUrl);

        let newBlock: number | null = null;
        const currentBlock = await alice._providerL2().getBlockNumber();

        // Pubsub notifier is not reactive + tests are being run in parallel, so we can't expect that the next block
        // would be expected one. Instead, we just want to receive an event with the particular block number.
        await wsProvider.on('block', (block) => {
            if (block >= currentBlock) {
                newBlock = block;
            }
        });

        const tx = await alice.transfer({
            to: alice.address,
            amount: 1,
            token: l2Token
        });

        let iterationsCount = 0;
        while (!newBlock) {
            await tryWait(iterationsCount++);
        }

        // More blocks may be created between the moment when the block was requested and event was received.
        expect(newBlock).toBeGreaterThanOrEqual(currentBlock);
        // ...though the gap should not be *too* big.
        expect(newBlock).toBeLessThan(currentBlock + 100);
        await tx.wait(); // To not leave a hanging promise.
        await wsProvider.destroy();
    });

    test('Should test pub-sub API: txs', async () => {
        // Checks that we can receive an event for new pending transactions.
        let wsProvider = new ethers.WebSocketProvider(testMaster.environment().wsL2NodeUrl);

        // We're sending a few transfers from the wallet, so we'll use a new account to make event unique.
        let uniqueRecipient = testMaster.newEmptyAccount().address;

        let newTxHash: string | null = null;
        // We can't use `once` as there may be other pending txs sent together with our one.
        await wsProvider.on('pending', async (txHash) => {
            const tx = await alice.provider.getTransaction(txHash);
            // We're waiting for the exact transaction to appear.
            if (!tx || tx.to != uniqueRecipient) {
                // Not the transaction we're looking for.
                return;
            }

            newTxHash = txHash;
        });

        const tx = await alice.transfer({
            to: uniqueRecipient,
            amount: 1,
            token: zksync.utils.L2_BASE_TOKEN_ADDRESS // With ERC20 "to" would be an address of the contract.
        });

        let iterationsCount = 0;
        while (!newTxHash) {
            await tryWait(iterationsCount++);
        }

        expect(newTxHash as string).toEqual(tx.hash);
        await tx.wait(); // To not leave a hanging promise.
        await wsProvider.destroy();
    });

    test('Should test pub-sub API: events', async () => {
        // Checks that we can receive an event for events matching a certain filter.
        let wsProvider = new ethers.WebSocketProvider(testMaster.environment().wsL2NodeUrl);
        let newEvent: Event | null = null;

        // We're sending a few transfers from the wallet, so we'll use a new account to make event unique.
        let uniqueRecipient = testMaster.newEmptyAccount().address;

        // Set up a filter for an ERC20 transfer.
        const erc20TransferTopic = ethers.id('Transfer(address,address,uint256)');
        let filter = {
            address: l2Token,
            topics: [
                erc20TransferTopic,
                ethers.zeroPadValue(alice.address, 32), // Filter only transfers from this wallet.,
                ethers.zeroPadValue(uniqueRecipient, 32) // Recipient
            ]
        };
        await wsProvider.once(filter, (event) => {
            newEvent = event;
        });

        // Set up a filter that should not match anything.
        let incorrectFilter = {
            address: alice.address
        };
        await wsProvider.once(incorrectFilter, (_) => {
            expect(null).fail('Found log for incorrect filter');
        });

        const tx = await alice.transfer({
            to: uniqueRecipient,
            amount: 1,
            token: l2Token
        });

        let iterationsCount = 0;
        while (!newEvent) {
            await tryWait(iterationsCount++);
        }

        expect((newEvent as any).transactionHash).toEqual(tx.hash);
        await tx.wait(); // To not leave a hanging promise.
        await wsProvider.destroy();
    });

    test('Should test transaction status', async () => {
        const amount = 1;
        const token = l2Token;

        const randomHash = ethers.hexlify(ethers.randomBytes(32));
        let status = await alice.provider.getTransactionStatus(randomHash);
        expect(status).toEqual(types.TransactionStatus.NotFound);

        const sentTx = await alice.transfer({
            to: alice.address,
            amount,
            token
        });

        do {
            status = await alice.provider.getTransactionStatus(sentTx.hash);
        } while (status == types.TransactionStatus.NotFound);

        status = await alice.provider.getTransactionStatus(sentTx.hash);
        expect(
            status == types.TransactionStatus.Processing || status == types.TransactionStatus.Committed
        ).toBeTruthy();

        await sentTx.wait();
        status = await alice.provider.getTransactionStatus(sentTx.hash);
        expect(status == types.TransactionStatus.Committed || status == types.TransactionStatus.Finalized).toBeTruthy();

        if (!testMaster.isFastMode()) {
            // It's not worth it to wait for finalization in the API test.
            // If it works on localhost, it *must* work elsewhere.
            await sentTx.waitFinalize();
            status = await alice.provider.getTransactionStatus(sentTx.hash);
            expect(status).toEqual(types.TransactionStatus.Finalized);
        }
    });

    test('Should test L2 transaction details', async () => {
        const amount = 1;
        const token = l2Token;

        const randomHash = ethers.hexlify(ethers.randomBytes(32));
        let details = await alice.provider.getTransactionDetails(randomHash);
        expect(details).toEqual(null);

        const sentTx = await alice.transfer({
            to: alice.address,
            amount,
            token
        });
        let expectedDetails = {
            fee: expect.stringMatching(HEX_VALUE_REGEX),
            gasPerPubdata: expect.stringMatching(HEX_VALUE_REGEX),
            initiatorAddress: alice.address.toLowerCase(),
            isL1Originated: false,
            receivedAt: expect.stringMatching(DATE_REGEX),
            status: expect.stringMatching(/pending|failed|included|verified/)
        };
        details = await alice.provider.getTransactionDetails(sentTx.hash);
        expect(details).toMatchObject(expectedDetails);

        const receipt = await sentTx.wait();
        expectedDetails.status = expect.stringMatching(/failed|included|verified/);

        details = await alice.provider.getTransactionDetails(receipt.hash);
        expect(details).toMatchObject(expectedDetails);

        if (!testMaster.isFastMode()) {
            // It's not worth it to wait for finalization in the API test.
            // If it works on localhost, it *must* work elsewhere.
            await sentTx.waitFinalize();
            details = await alice.provider.getTransactionDetails(receipt.hash);
            expectedDetails.status = expect.stringMatching(/verified/);
            expect(details).toMatchObject(expectedDetails);
        }
    });

    test('Should test L1 transaction details', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        let amount = 1;

        const sentTx = await alice.deposit({
            token: zksync.utils.ETH_ADDRESS,
            amount,
            approveBaseERC20: true
        });
        const receipt = await sentTx.wait();

        let details = await alice.provider.getTransactionDetails(receipt.hash);

        let expectedDetails = {
            fee: expect.stringMatching(HEX_VALUE_REGEX),
            gasPerPubdata: expect.stringMatching(HEX_VALUE_REGEX),
            initiatorAddress: expect.stringMatching(HEX_VALUE_REGEX),
            isL1Originated: true,
            receivedAt: expect.stringMatching(DATE_REGEX),
            status: expect.stringMatching(/failed|included|verified/)
        };
        expect(details).toMatchObject(expectedDetails);
    });

    test('Should check miniblock range', async () => {
        const l1BatchNumber = (await alice.provider.getL1BatchNumber()) - 2;
        const range = await alice.provider.getL1BatchBlockRange(l1BatchNumber);
        expect(range).toBeTruthy();

        const [from, to] = range!;

        for (let i = from; i <= to; i++) {
            const block = await alice.provider.getBlock(i, true);
            expect(block.l1BatchNumber).toEqual(l1BatchNumber);
            expect(block.l1BatchTimestamp).toEqual(expect.anything());
            expect(block.number).toEqual(i);
            for (let tx of block.prefetchedTransactions) {
                expect(tx.blockNumber).toEqual(i);
                const receipt = await alice.provider.getTransactionReceipt(tx.hash);
                expect(receipt!.l1BatchNumber).toEqual(l1BatchNumber);
            }
        }

        const prevBlock = await alice.provider.getBlock(from - 1, true);
        expect(prevBlock.l1BatchNumber).toEqual(l1BatchNumber - 1);

        const nextBlock = await alice.provider.getBlock(to + 1);
        expect(nextBlock.l1BatchNumber).toEqual(l1BatchNumber + 1);
    });

    // TODO (SMA-1576): This test is flaky. Test logic seems to be correct: we *first*
    // subscribe for events and then send transactions. However, this test
    // sometimes fails because one of the events was not received. Probably, there is
    // some problem in the pub-sub API that should be found & fixed.
    test('Should listen for human-readable events', async () => {
        const contract = await deployContract(alice, contracts.events, []);

        const blockNumber = await alice.provider.getBlockNumber();
        const deadbeef = ethers.zeroPadValue('0xdeadbeef', 20);
        const c0ffee = ethers.zeroPadValue('0xc0ffee', 20);
        const emitted = {
            trivial: 0,
            simple: 0,
            indexed: 0
        };

        contract.connect(alice);
        (
            await (
                await contract.on(contract.filters.Trivial(), () => ++emitted.trivial)
            ).on(contract.filters.Simple(), (_number: any, address: any) => {
                ++emitted.simple;
                expect(address.toLowerCase()).toEqual(deadbeef);
            })
        ).on(contract.filters.Indexed(42), (number: any, address: any) => {
            ++emitted.indexed;
            expect(number.toNumber()).toEqual(42);
            expect(address.toLowerCase()).toEqual(c0ffee);
        });

        let tx = await contract.test(42);
        await tx.wait();
        tx = await contract.test(18);
        await tx.wait();

        // Pubsub notify is not reactive and may be laggy, so we want to increase the chances
        // for test to pass. So we try to sleep a few iterations until we receive expected amount
        // of events. If we don't receive them, we continue and the test will fail anyway.
        const expectedTrivialEventsCount = 2;
        const expectedSimpleEventsCount = 2;
        const expectedIndexedEventsCount = 1;

        for (let iter = 0; iter < 20; iter++) {
            if (
                emitted.trivial >= expectedTrivialEventsCount &&
                emitted.simple >= expectedSimpleEventsCount &&
                emitted.indexed >= expectedIndexedEventsCount
            ) {
                break;
            }
            await zksync.utils.sleep(alice.provider.pollingInterval);
        }

        let events = await contract.queryFilter(contract.filters.Trivial(), blockNumber);
        expect(events).toHaveLength(expectedTrivialEventsCount);
        events = await contract.queryFilter(contract.filters.Simple(), blockNumber);
        expect(events).toHaveLength(expectedSimpleEventsCount);
        events = await contract.queryFilter(contract.filters.Indexed(42), blockNumber);
        expect(events).toHaveLength(1);

        expect(emitted.trivial).toEqual(expectedTrivialEventsCount);
        expect(emitted.simple).toEqual(expectedSimpleEventsCount);
        expect(emitted.indexed).toEqual(expectedIndexedEventsCount);

        contract.removeAllListeners();
    });

    test('Should check metamask interoperability', async () => {
        // Prepare "metamask" wallet.
        const from = new MockMetamask(alice, chainId);
        const to = alice.address;
        const browserProvider = new zksync.BrowserProvider(from);
        const signer = zksync.Signer.from(await browserProvider.getSigner(), Number(chainId), alice.provider);

        // Check to ensure that tx was correctly processed.
        const feeCheck = await shouldOnlyTakeFee(alice);

        // Ensure that transaction is accepted by the server.
        const transfer = signer.transfer({
            to,
            token: l2Token,
            amount: 1,
            overrides: {
                // We're testing sending an EIP712 tx with metamask.
                type: zksync.utils.EIP712_TX_TYPE
            }
        });
        await expect(transfer).toBeAccepted([feeCheck]);
    });

    test('Should check API accepts JSON RPC requests with additional fields', async () => {
        const req: RequestInit = {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                jsonrpc: '2.0',
                method: 'eth_blockNumber',
                params: [],
                id: 1,
                someExtraField: true
            })
        };

        await expect(
            fetch(testMaster.environment().l2NodeUrl, req).then((response) => response.json())
        ).resolves.toHaveProperty('result', expect.stringMatching(HEX_VALUE_REGEX));
    });

    test('Should throw error for estimate gas for account with balance < tx.value', async () => {
        let poorBob = testMaster.newEmptyAccount();
        await expect(
            poorBob.estimateGas({ value: 1, to: alice.address })
        ).toBeRejected(/*'insufficient balance for transfer'*/);
    });

    test('Should check API returns correct block for every tag', async () => {
        const earliestBlock = await alice.provider.send('eth_getBlockByNumber', ['earliest', true]);
        expect(+earliestBlock.number!).toEqual(0);
        const committedBlock = await alice.provider.send('eth_getBlockByNumber', ['committed', true]);
        expect(+committedBlock.number!).toEqual(expect.any(Number));
        const finalizedBlock = await alice.provider.send('eth_getBlockByNumber', ['finalized', true]);
        expect(+finalizedBlock.number!).toEqual(expect.any(Number));
        const latestBlock = await alice.provider.send('eth_getBlockByNumber', ['latest', true]);
        expect(+latestBlock.number!).toEqual(expect.any(Number));
        const l1CommittedBlock = await alice.provider.send('eth_getBlockByNumber', ['l1_committed', true]);
        expect(+l1CommittedBlock.number!).toEqual(expect.any(Number));
        const pendingBlock = await alice.provider.send('eth_getBlockByNumber', ['pending', true]);
        expect(pendingBlock).toEqual(null);
    });

    test('Should check sendRawTransaction returns GasPerPubDataLimitZero with 0 gas_per_pubdata_limit', async () => {
        const gasPrice = await alice.provider.getGasPrice();
        const chainId = (await alice.provider.getNetwork()).chainId;
        const address = zksync.Wallet.createRandom().address;
        const senderNonce = await alice.getNonce();
        const tx: ethers.TransactionRequest = {
            to: address,
            from: alice.address,
            nonce: senderNonce,
            gasLimit: 300000n,
            gasPrice,
            data: '0x',
            value: 0,
            chainId,
            type: 113,
            customData: {
                gasPerPubdata: '0'
            }
        };

        await expect(alice.sendTransaction(tx)).toBeRejected('gas per pub data limit is zero');
    });

    test('Should check getLogs works with address/topics in filter', async () => {
        // We're sending a transfer from the wallet, so we'll use a new account to make event unique.
        let uniqueRecipient = testMaster.newEmptyAccount().address;
        const tx = await alice.transfer({
            to: uniqueRecipient,
            amount: 1,
            token: l2Token
        });
        const receipt = await tx.wait();
        const logs = await alice.provider.getLogs({
            fromBlock: receipt.blockNumber,
            toBlock: receipt.blockNumber,
            address: l2Token,
            topics: [
                '0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef',
                ethers.zeroPadValue(alice.address, 32),
                ethers.zeroPadValue(uniqueRecipient, 32)
            ]
        });
        expect(logs).toHaveLength(1);
        expect(logs[0].transactionHash).toEqual(tx.hash);
    });

    test('Should check getLogs returns block_timestamp', async () => {
        // We're sending a transfer from the wallet, so we'll use a new account to make event unique.
        let uniqueRecipient = testMaster.newEmptyAccount().address;
        const tx = await alice.transfer({
            to: uniqueRecipient,
            amount: 1,
            token: l2Token
        });
        const receipt = await tx.wait();
        const response = await alice.provider.send('eth_getLogs', [
            {
                fromBlock: ethers.toBeHex(receipt.blockNumber),
                toBlock: ethers.toBeHex(receipt.blockNumber),
                address: l2Token,
                topics: [
                    '0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef',
                    ethers.zeroPadValue(alice.address, 32),
                    ethers.zeroPadValue(uniqueRecipient, 32)
                ]
            }
        ]);
        expect(response).toHaveLength(1);
        // TODO: switch to provider.getLogs once blockTimestamp is added to zksync ethers.js
        expect(response[0].blockTimestamp).toBeDefined();
    });

    test('Should check getLogs endpoint works properly with block tags', async () => {
        const earliestLogs = alice.provider.send('eth_getLogs', [
            {
                fromBlock: 'earliest',
                // we have to set this parameter to avoid `Query returned more than 10000 results. Try with this block range` error
                toBlock: '1'
            }
        ]);
        await expect(earliestLogs).resolves.not.toThrow();

        const committedLogs = alice.provider.send('eth_getLogs', [
            {
                fromBlock: 'committed',
                address: alice.address
            }
        ]);
        await expect(committedLogs).resolves.not.toThrow();

        const finalizedLogs = alice.provider.send('eth_getLogs', [
            {
                fromBlock: 'finalized',
                address: alice.address
            }
        ]);
        await expect(finalizedLogs).resolves.not.toThrow();

        const latestLogs = alice.provider.send('eth_getLogs', [
            {
                fromBlock: 'latest',
                address: alice.address
            }
        ]);
        await expect(latestLogs).resolves.not.toThrow();
    });

    test('Should check getLogs endpoint works properly with blockHash', async () => {
        let latestBlock = await alice.provider.getBlock('latest');

        // Check API returns identical logs by block number and block hash.
        // Logs can have different `l1BatchNumber` field though,
        // if L1 batch was sealed in between API requests are processed.
        const getLogsByNumber = (
            await alice.provider.getLogs({
                fromBlock: latestBlock.number,
                toBlock: latestBlock.number
            })
        ).map((x) => {
            return new zksync.types.Log({ ...x, l1BatchNumber: 0 }, alice.provider); // Set bogus value.
        });
        const getLogsByHash = (await alice.provider.getLogs({ blockHash: latestBlock.hash || undefined })).map((x) => {
            return new zksync.types.Log({ ...x, l1BatchNumber: 0 }, alice.provider); // Set bogus value.
        });
        expect(getLogsByNumber).toEqual(getLogsByHash);

        // Check that incorrect queries are rejected.
        await expect(
            alice.provider.getLogs({
                fromBlock: latestBlock.number,
                toBlock: latestBlock.number,
                blockHash: latestBlock.hash || undefined
            })
        ).rejects.toThrow(`invalid filter`);
    });

    test('Should check eth_feeHistory', async () => {
        const receipt = await anyTransaction(alice);
        const response = await alice.provider.send('eth_feeHistory', ['0x2', ethers.toBeHex(receipt.blockNumber), []]);

        expect(parseInt(response.oldestBlock)).toEqual(receipt.blockNumber - 1);

        expect(response.baseFeePerGas).toHaveLength(3);
        expect(response.baseFeePerBlobGas).toHaveLength(3);
        expect(response.gasUsedRatio).toHaveLength(2);
        expect(response.blobGasUsedRatio).toHaveLength(2);
        expect(response.l2PubdataPrice).toHaveLength(2);
        for (let i = 0; i < 2; i += 1) {
            const expectedBaseFee = (await alice.provider.getBlock(receipt.blockNumber - 1 + i)).baseFeePerGas;
            expect(BigInt(response.baseFeePerGas[i])).toEqual(expectedBaseFee);
        }
    });

    test('Should check zks_getProtocolVersion endpoint', async () => {
        const latestProtocolVersion = await alice.provider.send('zks_getProtocolVersion', []);
        let expectedSysContractsHashes = {
            bootloader: expect.stringMatching(HEX_VALUE_REGEX),
            default_aa: expect.stringMatching(HEX_VALUE_REGEX)
        };
        let expectedProtocolVersion = {
            version_id: expect.any(Number),
            minorVersion: expect.any(Number),
            base_system_contracts: expectedSysContractsHashes,
            bootloaderCodeHash: expect.stringMatching(HEX_VALUE_REGEX),
            defaultAccountCodeHash: expect.stringMatching(HEX_VALUE_REGEX),
            verification_keys_hashes: {
                recursion_scheduler_level_vk_hash: expect.stringMatching(HEX_VALUE_REGEX)
            },
            timestamp: expect.any(Number)
        };
        expect(latestProtocolVersion).toMatchObject(expectedProtocolVersion);

        const exactProtocolVersion = await alice.provider.send('zks_getProtocolVersion', [
            latestProtocolVersion.minorVersion
        ]);
        expect(exactProtocolVersion).toMatchObject(expectedProtocolVersion);
    });

    test('Should check transaction signature for legacy transaction type', async () => {
        const value = 1;
        const gasLimit = 350000;
        const gasPrice = await alice.provider.getGasPrice();
        const data = '0x';
        const to = alice.address;

        const LEGACY_TX_TYPE = 0;
        const legacyTxReq = {
            type: LEGACY_TX_TYPE,
            to,
            value,
            chainId,
            gasLimit,
            gasPrice,
            data,
            nonce: await alice.getNonce()
        };
        const signedLegacyTx = await alice.signTransaction(legacyTxReq);
        const tx_handle = await alice.provider.broadcastTransaction(signedLegacyTx);
        await tx_handle.wait();

        const txFromApi = await alice.provider.getTransaction(tx_handle.hash);

        const serializedLegacyTxReq = ethers.Transaction.from(legacyTxReq).unsignedSerialized;

        // check that API returns correct signature values for the given transaction
        // by invoking recoverAddress() method with the serialized transaction and signature values
        const signerAddr = ethers.recoverAddress(ethers.keccak256(serializedLegacyTxReq), txFromApi.signature);
        expect(signerAddr).toEqual(alice.address);

        const expectedV = 35n + BigInt(chainId) * 2n;
        const actualV = ethers.Signature.getChainIdV(chainId, txFromApi.signature.v);
        expect(actualV === expectedV);
    });

    test('Should check transaction signature for EIP1559 transaction type', async () => {
        const value = 1;
        const gasLimit = 350000;
        const gasPrice = await alice.provider.getGasPrice();
        const data = '0x';
        const to = alice.address;

        const EIP1559_TX_TYPE = 2;
        const eip1559TxReq = {
            type: EIP1559_TX_TYPE,
            to,
            value,
            chainId,
            gasLimit,
            data,
            nonce: await alice.getNonce(),
            maxFeePerGas: gasPrice,
            maxPriorityFeePerGas: gasPrice
        };

        const signedEip1559TxReq = await alice.signTransaction(eip1559TxReq);
        const tx_handle = await alice.provider.broadcastTransaction(signedEip1559TxReq);
        await tx_handle.wait();

        const txFromApi = await alice.provider.getTransaction(tx_handle.hash);

        const serializedEip1559TxReq = ethers.Transaction.from(eip1559TxReq).unsignedSerialized;

        // check that API returns correct signature values for the given transaction
        // by invoking recoverAddress() method with the serialized transaction and signature values
        const signerAddr = ethers.recoverAddress(ethers.keccak256(serializedEip1559TxReq), txFromApi.signature);
        expect(signerAddr).toEqual(alice.address);
        expect(txFromApi.signature.v! === 27 || 28);
    });

    describe('Storage override', () => {
        test('Should be able to estimate_gas overriding the balance of the sender', async () => {
            const balance = await alice.getBalance();
            const amount = balance + 1n;

            // Expect the transaction to be reverted without the overridden balance
            await expect(
                alice.provider.estimateGas({
                    from: alice.address,
                    to: alice.address,
                    value: amount.toString()
                })
            ).toBeRejected();

            // Call estimate_gas overriding the balance of the sender using the eth_estimateGas endpoint
            const response = await alice.provider.send('eth_estimateGas', [
                {
                    from: alice.address,
                    to: alice.address,
                    value: amount.toString()
                },
                'latest',
                //override with the balance needed to send the transaction
                {
                    [alice.address]: {
                        balance: amount.toString()
                    }
                }
            ]);

            // Assert that the response is successful
            expect(response).toEqual(expect.stringMatching(HEX_VALUE_REGEX));
        });
        test('Should be able to estimate_gas overriding contract code', async () => {
            // Deploy the first contract
            const contract1 = await deployContract(alice, contracts.events, []);
            const contract1Address = await contract1.getAddress();

            // Deploy the second contract to extract the code that we are overriding the estimation with
            const contract2 = await deployContract(alice, contracts.counter, []);
            const contract2Address = await contract2.getAddress();

            // Get the code of contract2
            const code = await alice.provider.getCode(contract2Address);

            // Get the calldata of the increment function of contract2
            const incrementFunctionData = contract2.interface.encodeFunctionData('increment', [1]);

            // Assert that the estimation fails because the increment function is not present in contract1
            await expect(
                alice.provider.estimateGas({
                    to: contract1Address.toString(),
                    data: incrementFunctionData
                })
            ).toBeRejected();

            // Call estimate_gas overriding the code of contract1 with the code of contract2 using the eth_estimateGas endpoint
            const response = await alice.provider.send('eth_estimateGas', [
                {
                    from: alice.address,
                    to: contract1Address.toString(),
                    data: incrementFunctionData
                },
                'latest',
                { [contract1Address.toString()]: { code: code } }
            ]);

            // Assert that the response is successful
            expect(response).toEqual(expect.stringMatching(HEX_VALUE_REGEX));
        });

        test('Should estimate gas by overriding state with State', async () => {
            const contract = await deployContract(alice, contracts.stateOverride, []);
            const contractAddress = await contract.getAddress();

            const sumValuesFunctionData = contract.interface.encodeFunctionData('sumValues', []);

            // Ensure that the initial gas estimation fails due to contract requirements
            await expect(
                alice.provider.estimateGas({
                    to: contractAddress.toString(),
                    data: sumValuesFunctionData
                })
            ).toBeRejected();

            // Override the entire contract state using State
            const state = {
                [contractAddress.toString()]: {
                    state: {
                        '0x0000000000000000000000000000000000000000000000000000000000000000':
                            '0x0000000000000000000000000000000000000000000000000000000000000001',
                        '0x0000000000000000000000000000000000000000000000000000000000000001':
                            '0x0000000000000000000000000000000000000000000000000000000000000002'
                    }
                }
            };

            const response = await alice.provider.send('eth_estimateGas', [
                {
                    from: alice.address,
                    to: contractAddress.toString(),
                    data: sumValuesFunctionData
                },
                'latest',
                state
            ]);

            expect(response).toEqual(expect.stringMatching(HEX_VALUE_REGEX));
        });

        test('Should estimate gas by overriding state with StateDiff', async () => {
            const contract = await deployContract(alice, contracts.stateOverride, []);
            const contractAddress = await contract.getAddress();
            const incrementFunctionData = contract.interface.encodeFunctionData('increment', [1]);

            // Ensure that the initial gas estimation fails due to contract requirements
            await expect(
                alice.provider.estimateGas({
                    to: contractAddress.toString(),
                    data: incrementFunctionData
                })
            ).toBeRejected();

            // Override the contract state using StateDiff
            const stateDiff = {
                [contractAddress.toString()]: {
                    stateDiff: {
                        '0x0000000000000000000000000000000000000000000000000000000000000000':
                            '0x0000000000000000000000000000000000000000000000000000000000000001'
                    }
                }
            };

            const response = await alice.provider.send('eth_estimateGas', [
                {
                    from: alice.address,
                    to: contractAddress.toString(),
                    data: incrementFunctionData
                },
                'latest',
                stateDiff
            ]);

            expect(response).toEqual(expect.stringMatching(HEX_VALUE_REGEX));
        });

        test('Should call and succeed with overriding state with State', async () => {
            const contract = await deployContract(alice, contracts.stateOverride, []);
            const contractAddress = await contract.getAddress();
            const sumValuesFunctionData = contract.interface.encodeFunctionData('sumValues', []);

            // Ensure that the initial call fails due to contract requirements
            await alice.provider
                .call({
                    to: contractAddress.toString(),
                    data: sumValuesFunctionData
                })
                .catch((error) => {
                    const errorString = 'Initial state not set';
                    expect(error.message).toContain(errorString);
                });

            // Override the contract state using State
            const state = {
                [contractAddress.toString()]: {
                    state: {
                        '0x0000000000000000000000000000000000000000000000000000000000000000':
                            '0x0000000000000000000000000000000000000000000000000000000000000001',
                        '0x0000000000000000000000000000000000000000000000000000000000000001':
                            '0x0000000000000000000000000000000000000000000000000000000000000002'
                    }
                }
            };

            const response = await alice.provider.send('eth_call', [
                {
                    from: alice.address,
                    to: contractAddress.toString(),
                    data: sumValuesFunctionData
                },
                'latest',
                state
            ]);

            // The state replace the entire state of the contract, so the sum now would be
            // 1 (0x1) + 2 (0x2) = 3 (0x3)
            expect(response).toEqual('0x0000000000000000000000000000000000000000000000000000000000000003');
        });

        test('Should call and succeed with overriding state with StateDiff', async () => {
            const contract = await deployContract(alice, contracts.stateOverride, []);
            const contractAddress = await contract.getAddress();
            const sumValuesFunctionData = contract.interface.encodeFunctionData('sumValues', []);

            // Ensure that the initial call fails due to contract requirements
            await alice.provider
                .call({
                    to: contractAddress.toString(),
                    data: sumValuesFunctionData
                })
                .catch((error) => {
                    const errorString = 'Initial state not set';
                    expect(error.message).toContain(errorString);
                });

            // Override the contract state using State
            const stateDiff = {
                [contractAddress.toString()]: {
                    stateDiff: {
                        '0x0000000000000000000000000000000000000000000000000000000000000000':
                            '0x0000000000000000000000000000000000000000000000000000000000000001',
                        '0x0000000000000000000000000000000000000000000000000000000000000001':
                            '0x0000000000000000000000000000000000000000000000000000000000000002'
                    }
                }
            };

            const response = await alice.provider.send('eth_call', [
                {
                    from: alice.address,
                    to: contractAddress.toString(),
                    data: sumValuesFunctionData
                },
                'latest',
                stateDiff
            ]);

            // The stateDiff only changes the specific slots provided in the override.
            // The initial value of the storage slot at key 0x2 remains unchanged, which is 100 (0x64 in hex).
            // Therefore, the sum of the values at the three storage slots is:
            // 1 (0x1) + 2 (0x2) + 100 (0x64) = 103 (0x67 in hex).
            // This is why the expected response is 0x67.
            expect(response).toEqual('0x0000000000000000000000000000000000000000000000000000000000000067');
        });
    });
    // We want to be sure that correct(outer) contract address is return in the transaction receipt,
    // when there is a contract that initializa another contract in the constructor
    test('Should check inner-outer contract address in the receipt of the deploy tx', async () => {
        const deploymentNonce = await alice.getDeploymentNonce();
        const expectedAddress = zksync.utils.createAddress(alice.address, deploymentNonce);

        const expectedBytecode = contracts.outer.bytecode;

        let innerContractBytecode = contracts.inner.bytecode;
        let outerContractOverrides = {
            customData: {
                factoryDeps: [innerContractBytecode]
            }
        };
        const outerContract = await deployContract(alice, contracts.outer, [1], undefined, outerContractOverrides);
        const contract = await outerContract.waitForDeployment();

        const deployedBytecode = await alice.provider.getCode(await contract.getAddress());

        expect(expectedAddress).toEqual(await contract.getAddress());
        expect(expectedBytecode).toEqual(deployedBytecode);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });

    /**
     * Waits a bit more. Fails the test if the `iterationStep` is too big.
     *
     * @param iterationStep The number of times this function has been called.
     */
    async function tryWait(iterationStep: number) {
        const maxWaitTimeMs = 100_000; // 100 seconds
        const maxRetries = maxWaitTimeMs / alice.provider.pollingInterval;
        await zksync.utils.sleep(alice.provider.pollingInterval);
        if (iterationStep >= maxRetries) {
            expect(null).fail(`Timeout while waiting for updates.`);
        }
    }
});

export class MockMetamask {
    readonly isMetaMask: boolean = true;
    readonly chainId: string;

    constructor(
        readonly wallet: zksync.Wallet,
        readonly networkVersion: bigint
    ) {
        this.chainId = ethers.toBeHex(networkVersion);
    }

    // EIP-1193
    async request(req: { method: string; params?: any[] }) {
        let { method, params } = req;
        params ??= [];

        switch (method) {
            case 'eth_requestAccounts':
            case 'eth_accounts':
                return [this.wallet.address];
            case 'net_version':
                return this.networkVersion;
            case 'eth_chainId':
                return this.chainId;
            case 'personal_sign':
                return this.wallet.signMessage(params[0]);
            case 'eth_sendTransaction':
                const tx = params[0];
                delete tx.gas;
                let populated = {
                    ...(await this.wallet.populateTransaction(tx)),
                    nonce: await this.wallet.getNonce()
                };
                delete populated.from;
                const signed = await this.wallet.signTransaction(populated);
                const response = await this.wallet.provider.broadcastTransaction(signed);
                return response.hash;
            case 'eth_getTransactionCount':
                return this.wallet.getNonce();
            case 'eth_signTypedData_v4':
                let payload = JSON.parse(params[1]);
                delete payload.types.EIP712Domain;
                return this.wallet.signTypedData(payload.domain, payload.types, payload.message);
            default:
                // unfortunately though, metamask does not forward methods from zks_ namespace
                if (method.startsWith('zks')) {
                    throw new Error('zks namespace methods are not forwarded by metamask');
                }
                return this.wallet.provider.send(method, params);
        }
    }
}

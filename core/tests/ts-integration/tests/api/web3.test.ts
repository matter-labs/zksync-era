/**
 * This suite contains tests for the Web3 API compatibility and zkSync-specific extensions.
 */
import { TestMaster } from '../../src';
import * as zksync from 'zksync-web3';
import { types } from 'zksync-web3';
import { ethers, Event } from 'ethers';
import { serialize } from '@ethersproject/transactions';
import { deployContract, getTestContract, waitForNewL1Batch, anyTransaction } from '../../src/helpers';
import { shouldOnlyTakeFee } from '../../src/modifiers/balance-checker';
import fetch, { RequestInit } from 'node-fetch';
import { EIP712_TX_TYPE, PRIORITY_OPERATION_L2_TX_TYPE } from 'zksync-web3/build/src/utils';
// Regular expression to match variable-length hex number.
const HEX_VALUE_REGEX = /^0x[\da-fA-F]*$/;
const DATE_REGEX = /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{6})?/;

const contracts = {
    counter: getTestContract('Counter'),
    events: getTestContract('Emitter')
};

describe('web3 API compatibility tests', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let l2Token: string;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        l2Token = testMaster.environment().erc20Token.l2Address;
    });

    test('Should test block/transaction web3 methods', async () => {
        const blockNumber = 1;
        const blockNumberHex = '0x1';

        // eth_getBlockByNumber
        const blockHash = (await alice.provider.getBlock(blockNumber)).hash;
        const blockWithTxsByNumber = await alice.provider.getBlockWithTransactions(blockNumber);
        expect(blockWithTxsByNumber.gasLimit).bnToBeGt(0);
        let sumTxGasUsed = ethers.BigNumber.from(0);

        for (const tx of blockWithTxsByNumber.transactions) {
            const receipt = await alice.provider.getTransactionReceipt(tx.hash);
            sumTxGasUsed = sumTxGasUsed.add(receipt.gasUsed);
        }
        expect(blockWithTxsByNumber.gasUsed).bnToBeGte(sumTxGasUsed);

        let expectedReceipts = [];

        for (const tx of blockWithTxsByNumber.transactions) {
            const receipt = await alice.provider.send('eth_getTransactionReceipt', [tx.hash]);
            expectedReceipts.push(receipt);
        }

        let receipts = await alice.provider.send('eth_getBlockReceipts', [blockNumberHex]);
        expect(receipts).toEqual(expectedReceipts);

        // eth_getBlockByHash
        await alice.provider.getBlock(blockHash);
        const blockWithTxsByHash = await alice.provider.getBlockWithTransactions(blockHash);
        expect(blockWithTxsByNumber.number).toEqual(blockWithTxsByHash.number);

        // eth_getBlockTransactionCountByNumber
        const txCountByNumber = await alice.provider.send('eth_getBlockTransactionCountByNumber', [blockNumberHex]);
        expect(parseInt(txCountByNumber, 16)).toEqual(blockWithTxsByNumber.transactions.length);

        // eth_getBlockTransactionCountByHash
        const txCountByHash = await alice.provider.send('eth_getBlockTransactionCountByHash', [blockHash]);
        expect(parseInt(txCountByHash, 16)).toEqual(blockWithTxsByNumber.transactions.length);

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
        const code = await alice.provider.getCode(counterContract.address);
        expect(code).toEqual(ethers.utils.hexlify(contracts.counter.bytecode));

        // eth_getStorageAt
        const accCodeStorageAddress = '0x0000000000000000000000000000000000008002';
        const codeKey = '0x000000000000000000000000' + counterContract.address.substring(2);
        const codeHash = await alice.provider.getStorageAt(accCodeStorageAddress, codeKey);

        const expectedHash = ethers.utils.sha256(contracts.counter.bytecode);
        expect(codeHash.substring(10)).toEqual(expectedHash.substring(10));
    });

    test('Should test some zks web3 methods', async () => {
        // zks_getAllAccountBalances
        // NOTE: `getAllBalances` will not work on external node,
        // since TokenListFetcher is not running
        if (!process.env.EN_MAIN_NODE_URL) {
            const balances = await alice.getAllBalances();
            const tokenBalance = await alice.getBalance(l2Token);
            expect(balances[l2Token.toLowerCase()].eq(tokenBalance));
        }
        // zks_L1ChainId
        const l1ChainId = (await alice.providerL1!.getNetwork()).chainId;
        const l1ChainIdFromL2Provider = await alice.provider.l1ChainId();
        expect(l1ChainId).toEqual(l1ChainIdFromL2Provider);
        // zks_getBlockDetails
        const blockDetails = await alice.provider.getBlockDetails(1);
        const block = await alice.provider.getBlock(1);
        expect(blockDetails.rootHash).toEqual(block.hash);
        expect(blockDetails.l1BatchNumber).toEqual(block.l1BatchNumber);
        // zks_getL1BatchDetails
        const batchDetails = await alice.provider.getL1BatchDetails(block.l1BatchNumber);
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
        // Valid network IDs for zkSync are greater than 270.
        // This test suite may run on different envs, so we don't expect a particular ID.
        await expect(alice.provider.send('net_version', [])).resolves.toMatch(/^27\d|28\d$/);
    });

    // @ts-ignore
    test.each([
        ['net_peerCount', [], '0x0'],
        ['net_listening', [], false],
        ['web3_clientVersion', [], 'zkSync/v2.0'],
        ['eth_protocolVersion', [], 'zks/1'],
        ['eth_syncing', [], false],
        ['eth_accounts', [], []],
        ['eth_coinbase', [], '0x0000000000000000000000000000000000000000'],
        ['eth_getCompilers', [], []],
        ['eth_hashrate', [], '0x0'],
        ['eth_mining', [], false],
        ['eth_getUncleCountByBlockNumber', ['0x0'], '0x0']
    ])('Should test bogus web3 methods (%s)', async (method: string, input: string[], output: string) => {
        await expect(alice.provider.send(method, input)).resolves.toEqual(output);
    });

    test('Should test bogus web3 methods (eth_getUncleCountByBlockHash)', async () => {
        // This test can't be represented as a part of the table, since the input is dynamic.
        const firstBlockHash = (await alice.provider.getBlock(1)).hash;
        await expect(alice.provider.send('eth_getUncleCountByBlockHash', [firstBlockHash])).resolves.toEqual('0x0');
    });

    test('Should test web3 response extensions', async () => {
        if (testMaster.isFastMode()) {
            // This test requires a new L1 batch to be created, which may be very time consuming on stage.
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
        const receipt = await alice.provider.getTransactionReceipt(tx.transactionHash);
        const logs = await alice.provider.getLogs({
            fromBlock: receipt.blockNumber,
            toBlock: receipt.blockNumber
        });
        const block = await alice.provider.getBlock(receipt.blockNumber);
        const blockWithTransactions = await alice.provider.getBlockWithTransactions(receipt.blockNumber);
        const tx1 = await alice.provider.getTransaction(tx.transactionHash);
        expect(tx1.l1BatchNumber).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
        expect(tx1.l1BatchTxIndex).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
        expect(tx1.chainId).toEqual(+process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!);
        expect(tx1.type).toEqual(0);

        expect(receipt.l1BatchNumber).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
        expect(receipt.l1BatchTxIndex).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
        expect(receipt.logs[0].l1BatchNumber).toEqual(receipt.l1BatchNumber);
        expect(logs[0].l1BatchNumber).toEqual(receipt.l1BatchNumber);
        expect(block.l1BatchNumber).toEqual(receipt.l1BatchNumber);
        expect(block.l1BatchTimestamp).toEqual(expect.anything());
        expect(blockWithTransactions.l1BatchNumber).toEqual(receipt.l1BatchNumber);
        expect(blockWithTransactions.l1BatchTimestamp).toEqual(expect.anything());
        blockWithTransactions.transactions.forEach((txInBlock, _) => {
            expect(txInBlock.l1BatchNumber).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
            expect(txInBlock.l1BatchTxIndex).toEqual(expect.anything()); // Can be anything except `null` or `undefined`.
            expect(txInBlock.chainId).toEqual(+process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!);
            expect([0, EIP712_TX_TYPE, PRIORITY_OPERATION_L2_TX_TYPE]).toContain(txInBlock.type);
        });
    });

    test('Should test various token methods', async () => {
        const tokens = await alice.provider.getConfirmedTokens();
        expect(tokens).not.toHaveLength(0); // Should not be an empty array.
    });

    test('Should check transactions from API / Legacy tx', async () => {
        const LEGACY_TX_TYPE = 0;
        const legacyTx = await alice.sendTransaction({
            type: LEGACY_TX_TYPE,
            to: alice.address
        });
        await legacyTx.wait();

        const legacyApiReceipt = await alice.provider.getTransaction(legacyTx.hash);
        expect(legacyApiReceipt.gasPrice).bnToBeEq(legacyTx.gasPrice!);
    });

    test('Should check transactions from API / EIP1559 tx', async () => {
        const EIP1559_TX_TYPE = 2;
        const eip1559Tx = await alice.sendTransaction({
            type: EIP1559_TX_TYPE,
            to: alice.address
        });
        await eip1559Tx.wait();

        const eip1559ApiReceipt = await alice.provider.getTransaction(eip1559Tx.hash);
        expect(eip1559ApiReceipt.maxFeePerGas).bnToBeEq(eip1559Tx.maxFeePerGas!);
        expect(eip1559ApiReceipt.maxPriorityFeePerGas).bnToBeEq(eip1559Tx.maxPriorityFeePerGas!);
    });

    test('Should test getFilterChanges for pending transactions', async () => {
        if (process.env.EN_MAIN_NODE_URL) {
            // Pending transactions logic doesn't work on EN since we don't have proper mempool -
            // transactions only appear in the DB after they are included in the block.
            return;
        }

        // We will need to wait until the mempool cache on the server is updated.
        // The default update period is 50 ms, so we will wait for 75 ms to be sure.
        const mempoolCacheWait = 50 + 25;

        let filterId = await alice.provider.send('eth_newPendingTransactionFilter', []);
        let changes = await alice.provider.send('eth_getFilterChanges', [filterId]);
        const tx1 = await alice.sendTransaction({
            to: alice.address
        });
        await zksync.utils.sleep(mempoolCacheWait);
        changes = await alice.provider.send('eth_getFilterChanges', [filterId]);
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
        await zksync.utils.sleep(mempoolCacheWait);
        changes = await alice.provider.send('eth_getFilterChanges', [filterId]);
        expect(changes).not.toContain(tx1.hash);
        expect(changes).toContain(tx2.hash);
        expect(changes).toContain(tx3.hash);
        expect(changes).toContain(tx4.hash);
    });

    test('Should test pub-sub API: blocks', async () => {
        // Checks that we can receive an event for new block being created.
        let wsProvider = new ethers.providers.WebSocketProvider(testMaster.environment().wsL2NodeUrl);

        let newBlock: number | null = null;
        const currentBlock = await alice._providerL2().getBlockNumber();

        // Pubsub notifier is not reactive + tests are being run in parallel, so we can't expect that the next block
        // would be expected one. Instead, we just want to receive an event with the particular block number.
        wsProvider.on('block', (block) => {
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
        wsProvider.removeAllListeners();
        await wsProvider.destroy();
    });

    test('Should test pub-sub API: txs', async () => {
        // Checks that we can receive an event for new pending transactions.
        let wsProvider = new ethers.providers.WebSocketProvider(testMaster.environment().wsL2NodeUrl);

        // We're sending a few transfers from the wallet, so we'll use a new account to make event unique.
        let uniqueRecipient = testMaster.newEmptyAccount().address;

        let newTxHash: string | null = null;
        // We can't use `once` as there may be other pending txs sent together with our one.
        wsProvider.on('pending', async (txHash) => {
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
            token: zksync.utils.ETH_ADDRESS // With ERC20 "to" would be an address of the contract.
        });

        let iterationsCount = 0;
        while (!newTxHash) {
            await tryWait(iterationsCount++);
        }

        expect(newTxHash as string).toEqual(tx.hash);
        await tx.wait(); // To not leave a hanging promise.
        wsProvider.removeAllListeners();
        await wsProvider.destroy();
    });

    test('Should test pub-sub API: events', async () => {
        // Checks that we can receive an event for events matching a certain filter.
        let wsProvider = new ethers.providers.WebSocketProvider(testMaster.environment().wsL2NodeUrl);
        let newEvent: Event | null = null;

        // We're sending a few transfers from the wallet, so we'll use a new account to make event unique.
        let uniqueRecipient = testMaster.newEmptyAccount().address;

        // Setup a filter for an ERC20 transfer.
        const erc20TransferTopic = ethers.utils.id('Transfer(address,address,uint256)');
        let filter = {
            address: l2Token,
            topics: [
                erc20TransferTopic,
                ethers.utils.hexZeroPad(alice.address, 32), // Filter only transfers from this wallet.,
                ethers.utils.hexZeroPad(uniqueRecipient, 32) // Recipient
            ]
        };
        wsProvider.once(filter, (event) => {
            newEvent = event;
        });

        // Setup a filter that should not match anything.
        let incorrectFilter = {
            address: alice.address
        };
        wsProvider.once(incorrectFilter, (_) => {
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

        expect((newEvent as any as Event).transactionHash).toEqual(tx.hash);
        await tx.wait(); // To not leave a hanging promise.
        wsProvider.removeAllListeners();
        await wsProvider.destroy();
    });

    test('Should test transaction status', async () => {
        const amount = 1;
        const token = l2Token;

        const randomHash = ethers.utils.hexlify(ethers.utils.randomBytes(32));
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

        const randomHash = ethers.utils.hexlify(ethers.utils.randomBytes(32));
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

        details = await alice.provider.getTransactionDetails(receipt.transactionHash);
        expect(details).toMatchObject(expectedDetails);

        if (!testMaster.isFastMode()) {
            // It's not worth it to wait for finalization in the API test.
            // If it works on localhost, it *must* work elsewhere.
            await sentTx.waitFinalize();
            details = await alice.provider.getTransactionDetails(receipt.transactionHash);
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
            amount
        });
        const receipt = await sentTx.wait();

        let details = await alice.provider.getTransactionDetails(receipt.transactionHash);

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
            const block = await alice.provider.getBlockWithTransactions(i);
            expect(block.l1BatchNumber).toEqual(l1BatchNumber);
            expect(block.l1BatchTimestamp).toEqual(expect.anything());
            expect(block.number).toEqual(i);
            for (let tx of block.transactions) {
                expect(tx.blockNumber).toEqual(i);
                const receipt = await alice.provider.getTransactionReceipt(tx.hash);
                expect(receipt.l1BatchNumber).toEqual(l1BatchNumber);
            }
        }

        const prevBlock = await alice.provider.getBlockWithTransactions(from - 1);
        expect(prevBlock.l1BatchNumber).toEqual(l1BatchNumber - 1);

        const nextBlock = await alice.provider.getBlock(to + 1);
        expect(nextBlock.l1BatchNumber).toEqual(l1BatchNumber + 1);
    });

    // TODO (SMA-1576): This test is flaky. Test logic seems to be correct: we *first*
    // subscribe for events and then send transactions. However, this test
    // sometimes fails because one of the events was not received. Probably, there is
    // some problem in the pub-sub API that should be found & fixed.
    test.skip('Should listen for human-readable events', async () => {
        const contract = await deployContract(alice, contracts.events, []);

        const blockNumber = await alice.provider.getBlockNumber();
        const deadbeef = ethers.utils.hexZeroPad('0xdeadbeef', 20);
        const c0ffee = ethers.utils.hexZeroPad('0xc0ffee', 20);
        const emitted = {
            trivial: 0,
            simple: 0,
            indexed: 0
        };

        contract.connect(alice);
        contract
            .on(contract.filters.Trivial(), () => ++emitted.trivial)
            .on(contract.filters.Simple(), (_number: any, address: any) => {
                ++emitted.simple;
                expect(address.toLowerCase()).toEqual(deadbeef);
            })
            .on(contract.filters.Indexed(42), (number: any, address: any) => {
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
        // of events. If we won't receive them, we continue and the test will fail anyway.
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
        const from = new MockMetamask(alice);
        const to = alice.address;
        const web3Provider = new zksync.Web3Provider(from);
        const signer = web3Provider.getSigner();

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

    test('Should check API returns error when there are too many logs in eth_getLogs', async () => {
        const contract = await deployContract(alice, contracts.events, []);
        const maxLogsLimit = parseInt(
            process.env.EN_REQ_ENTITIES_LIMIT ?? process.env.API_WEB3_JSON_RPC_REQ_ENTITIES_LIMIT!
        );

        // Send 3 transactions that emit `maxLogsLimit / 2` events.
        const tx1 = await contract.emitManyEvents(maxLogsLimit / 2);
        const tx1Receipt = await tx1.wait();

        const tx2 = await contract.emitManyEvents(maxLogsLimit / 2);
        await tx2.wait();

        const tx3 = await contract.emitManyEvents(maxLogsLimit / 2);
        const tx3Receipt = await tx3.wait();

        // There are around `0.5 * maxLogsLimit` logs in [tx1Receipt.blockNumber, tx1Receipt.blockNumber] range,
        // so query with such filter should succeed.
        await expect(alice.provider.getLogs({ fromBlock: tx1Receipt.blockNumber, toBlock: tx1Receipt.blockNumber }))
            .resolves;

        // There are at least `1.5 * maxLogsLimit` logs in [tx1Receipt.blockNumber, tx3Receipt.blockNumber] range,
        // so query with such filter should fail.
        await expect(
            alice.provider.getLogs({ fromBlock: tx1Receipt.blockNumber, toBlock: tx3Receipt.blockNumber })
        ).rejects.toThrow(`Query returned more than ${maxLogsLimit} results.`);
    });

    test('Should throw error for estimate gas for account with balance < tx.value', async () => {
        let poorBob = testMaster.newEmptyAccount();
        expect(poorBob.estimateGas({ value: 1, to: alice.address })).toBeRejected('insufficient balance for transfer');
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
        const pendingBlock = await alice.provider.send('eth_getBlockByNumber', ['pending', true]);
        expect(pendingBlock).toEqual(null);
    });

    test('Should check sendRawTransaction returns GasPerPubDataLimitZero with 0 gas_per_pubdata_limit', async () => {
        const gasPrice = await alice.provider.getGasPrice();
        const chainId = (await alice.provider.getNetwork()).chainId;
        const address = zksync.Wallet.createRandom().address;
        const senderNonce = await alice.getTransactionCount();
        const tx: ethers.providers.TransactionRequest = {
            to: address,
            from: alice.address,
            nonce: senderNonce,
            gasLimit: ethers.BigNumber.from(300000),
            data: '0x',
            value: 0,
            chainId,
            type: 113,
            maxPriorityFeePerGas: gasPrice,
            maxFeePerGas: gasPrice,
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
                ethers.utils.hexZeroPad(alice.address, 32),
                ethers.utils.hexZeroPad(uniqueRecipient, 32)
            ]
        });
        expect(logs).toHaveLength(1);
        expect(logs[0].transactionHash).toEqual(tx.hash);
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
            x.l1BatchNumber = 0; // Set bogus value.
            return x;
        });
        const getLogsByHash = (
            await alice.provider.getLogs({
                blockHash: latestBlock.hash
            })
        ).map((x) => {
            x.l1BatchNumber = 0; // Set bogus value.
            return x;
        });
        await expect(getLogsByNumber).toEqual(getLogsByHash);

        // Check that incorrect queries are rejected.
        await expect(
            alice.provider.getLogs({
                fromBlock: latestBlock.number,
                toBlock: latestBlock.number,
                blockHash: latestBlock.hash
            })
        ).rejects.toThrow(`invalid filter: if blockHash is supplied fromBlock and toBlock must not be`);
    });

    test('Should check eth_feeHistory', async () => {
        const receipt = await anyTransaction(alice);
        const response = await alice.provider.send('eth_feeHistory', [
            '0x2',
            ethers.utils.hexlify(receipt.blockNumber),
            []
        ]);

        expect(ethers.BigNumber.from(response.oldestBlock).toNumber()).toEqual(receipt.blockNumber - 1);

        expect(response.baseFeePerGas).toHaveLength(3);
        for (let i = 0; i < 2; i += 1) {
            const expectedBaseFee = (await alice.provider.getBlock(receipt.blockNumber - 1 + i)).baseFeePerGas;
            expect(ethers.BigNumber.from(response.baseFeePerGas[i])).toEqual(expectedBaseFee);
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
            base_system_contracts: expectedSysContractsHashes,
            verification_keys_hashes: {
                params: {
                    recursion_circuits_set_vks_hash: expect.stringMatching(HEX_VALUE_REGEX),
                    recursion_leaf_level_vk_hash: expect.stringMatching(HEX_VALUE_REGEX),
                    recursion_node_level_vk_hash: expect.stringMatching(HEX_VALUE_REGEX)
                },
                recursion_scheduler_level_vk_hash: expect.stringMatching(HEX_VALUE_REGEX)
            },
            timestamp: expect.any(Number)
        };
        expect(latestProtocolVersion).toMatchObject(expectedProtocolVersion);

        const exactProtocolVersion = await alice.provider.send('zks_getProtocolVersion', [
            latestProtocolVersion.version_id
        ]);
        expect(exactProtocolVersion).toMatchObject(expectedProtocolVersion);
    });

    test('Should check transaction signature', async () => {
        const CHAIN_ID = +process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!;
        const value = 1;
        const gasLimit = 350000;
        const gasPrice = await alice.provider.getGasPrice();
        const data = '0x';
        const to = alice.address;

        let tx_handle;
        let txFromApi;
        let signerAddr;

        // check for legacy transaction type
        const LEGACY_TX_TYPE = 0;
        const legacyTxReq = {
            type: LEGACY_TX_TYPE,
            to,
            value,
            chainId: CHAIN_ID,
            gasLimit,
            gasPrice,
            data,
            nonce: await alice.getTransactionCount()
        };
        const signedLegacyTx = await alice.signTransaction(legacyTxReq);
        tx_handle = await alice.provider.sendTransaction(signedLegacyTx);
        await tx_handle.wait();

        txFromApi = await alice.provider.getTransaction(tx_handle.hash);

        const serializedLegacyTxReq = ethers.utils.serializeTransaction(legacyTxReq);

        // check that API returns correct signature values for the given transaction
        // by invoking recoverAddress() method with the serialized transaction and signature values
        signerAddr = ethers.utils.recoverAddress(ethers.utils.keccak256(serializedLegacyTxReq), {
            r: txFromApi.r!,
            s: txFromApi.s!,
            v: txFromApi.v!
        });
        expect(signerAddr).toEqual(alice.address);

        const expectedV = 35 + CHAIN_ID! * 2;
        expect(Math.abs(txFromApi.v! - expectedV) <= 1).toEqual(true);

        // check for EIP1559 transaction type
        const EIP1559_TX_TYPE = 2;
        const eip1559TxReq = {
            type: EIP1559_TX_TYPE,
            to,
            value,
            chainId: CHAIN_ID,
            gasLimit,
            data,
            nonce: await alice.getTransactionCount(),
            maxFeePerGas: gasPrice,
            maxPriorityFeePerGas: gasPrice
        };

        const signedEip1559TxReq = await alice.signTransaction(eip1559TxReq);
        tx_handle = await alice.provider.sendTransaction(signedEip1559TxReq);
        await tx_handle.wait();

        txFromApi = await alice.provider.getTransaction(tx_handle.hash);

        const serializedEip1559TxReq = ethers.utils.serializeTransaction(eip1559TxReq);

        // check that API returns correct signature values for the given transaction
        // by invoking recoverAddress() method with the serialized transaction and signature values
        signerAddr = ethers.utils.recoverAddress(ethers.utils.keccak256(serializedEip1559TxReq), {
            r: txFromApi.r!,
            s: txFromApi.s!,
            v: txFromApi.v!
        });
        expect(signerAddr).toEqual(alice.address);
        expect(txFromApi.v! <= 1).toEqual(true);
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
    readonly networkVersion = parseInt(process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!, 10);
    readonly chainId: string = ethers.utils.hexlify(parseInt(process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!, 10));

    constructor(readonly wallet: zksync.Wallet) {}

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
                    nonce: await this.wallet.getTransactionCount()
                };
                delete populated.from;
                const signature = this.wallet._signingKey().signDigest(ethers.utils.keccak256(serialize(populated)));
                const signed = serialize(populated, signature);
                const response = await this.wallet.provider.sendTransaction(signed);
                return response.hash;
            case 'eth_getTransactionCount':
                return this.wallet.getTransactionCount();
            case 'eth_signTypedData_v4':
                let payload = JSON.parse(params[1]);
                delete payload.types.EIP712Domain;
                return this.wallet._signTypedData(payload.domain, payload.types, payload.message);
            default:
                // unfortunately though, metamask does not forward methods from zks_ namespace
                if (method.startsWith('zks')) {
                    throw new Error('zks namespace methods are not forwarded by metamask');
                }
                return this.wallet.provider.send(method, params);
        }
    }
}

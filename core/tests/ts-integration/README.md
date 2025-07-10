# NFTF -- New Fancy Test Framework

This folder contains a framework for writing integration tests for ZKsync Era, as well as set of integration test
suites.

This framework is built atop of [jest](https://jestjs.io/). It is _highly recommended_ to familiarize yourself with its
documentation to be able to write tests.

## Features

- Separated framework/tests logic.
- Parallel test suite execution.
- Ability to run tests concurrently within one suite.
- Custom matchers & helpers.
- Automated used funds recollection.

### Test run lifecycle

Before starting any test, framework would analyze `tests` folder to pick up all the test suites we have, and then will
prepare the context for tests. Context initialization consists of:

- Waiting for server to start.
- Creating personal accounts for each test suite.
- Providing funds to these accounts.

Basically, during initialization, everything is prepared for writing tests that interact with ZKsync.

After that, each test suite is ran _in parallel_. Each test suite can claim its own account and be sure that this
account has funds on it and is not used by any other suite.

After all the test suites are completed, funds from all the used accounts are recollected and sent back to the main
account used to setup the test.

### Sample test suite

To create a new test suite, you just need to create a file named `<whatever>.test.ts` in the `tests` directory.

Sample test suite would look like this:

```typescript
/**
 * This suite contains tests checking our handling of Ether (such as depositing, checking `msg.value`, etc).
 */

import { TestMaster } from "../src/index";
import { shouldChangeETHBalances } from "../src/modifiers/balance-checker.ts";

import * as zksync from "zksync-ethers";
import { BigNumber } from "ethers";

describe("ETH token checks", () => {
  let testMaster: TestMaster;
  let alice: zksync.Wallet;
  let bob: zksync.Wallet;

  beforeAll(() => {
    // Test master is an interface to access prefunded account.
    testMaster = TestMaster.getInstance(__filename);
    // ...through it you can obtain a personal funded account.
    alice = testMaster.mainAccount();
    // ...and an unlimited amount of empty ones.
    bob = testMaster.newEmptyAccount();
  });

  test("Can perform a transfer", async () => {
    const value = BigNumber.from(200);

    // Declare modifier to check that ETH balances would change in the following way.
    const ethBalanceChange = await shouldChangeETHBalances([
      { wallet: alice, change: -value },
      { wallet: bob, change: value },
    ]);

    // Send transfer, it should succeed. Apply a modifier we declared above.
    await expect(
      alice.sendTransaction({ to: bob.address, value }),
    ).toBeAccepted([ethBalanceChange]);
  });

  afterAll(async () => {
    // It will collect all the funds from temporary accounts back.
    await testMaster.deinitialize();
  });
});
```

### Custom matchers

One of the big features of this framework is the ability to create custom matchers to not duplicate the checking logic
that is used in multiple tests.

To see documentation on the matchers available by default, see the [jest documentation](https://jestjs.io/docs/expect).
To see documentation on the custom matchers of this framework, see [the type definitions file](./typings/jest.d.ts).

To declare your own matcher, look at [existing matchers](./src/matchers/) and use them as an example. Once matchers are
implemented, register them at [setup file](./src/jest-setup/add-matchers.ts) and add type declarations to the
[typings file](./typings/jest.d.ts). Don't forget to add documentation for the matcher!

### Matcher modifiers

`toBeAccepted` and `toBeRejected` matchers accept modifiers. You can see one (`shouldChangeETHBalances`) above. There
are others (like `shouldChangeTokenBalances` or `shouldOnlyTakeFee`), and if needed you can create your own ones.

These modifiers would be applied to the transaction receipt, and you can implement any kind of custom logic there. To do
so, you just need to declare a class that inherits `MatcherModifier` class and implements the `check` method.

For more details on the interface, see the [index.ts](./src/modifiers/index.ts).

Note: you don't have to always declare modifiers there. If your modifier is specific to one suite only, you can declare
it right there and use in your tests.

## Helpers

For common actions like deploying a contract or transferring funds, a set of helpers is provided. These helpers can be
found in the [corresponding file](./src/helpers.ts).

Feel free to add new functionality there if it'll be used by multiple test suites. If the logic is exclusive for just
one test suite, you may declare your own helper functions right in the suite file.

## Debug logging

During context initialization and teardown you can enable showing the debug logs to troubleshoot issues. To do so, run
tests with the `ZKSYNC_DEBUG_LOGS` environment variable set.

To add more logs to the context initialization, use `reporter` object in the `ContextOwner` class. Example:

```typescript
this.reporter.debug(`Some useful info`);
```

## Fast and long modes

Some integration tests may be very long to run in real environment (e.g. stage). Namely, tests that wait for the block
finalization: it make take several hours to generate a proof and send it onchain.

Because of that, framework supports "fast" and "long" modes. `TestMaster` objects have `isFastMode` method to determine
which mode is currently being used.

If you're going to write a test that can make test run duration longer, it is advised to guard the "long" part with the
corresponding check.

By default, "long" mode is assumed, and to enable the "fast" mode one must set the `ZK_INTEGRATION_TESTS_FAST_MODE`
environment variable to `true`.

## Helpful patterns

### Checking promise and then using it

Sometimes you need to check promise in a matcher, but then you need to actually use the resolved result. Solution: in
JS/TS it's OK to await the promise twice.

You can do it as follows:

```typescript
// Create promise
const withdrawalPromise = alice.withdraw({
  token: zksync.utils.ETH_ADDRESS,
  amount,
});
// Here you use promise in the matcher.
await expect(withdrawalPromise).toBeAccepted([l2ethBalanceChange]);
// And here you retrieve its value. It's OK to await same promise twice.
const withdrawalTx = await withdrawalPromise;
await withdrawalTx.waitFinalize();
```

### Focus on what you're checking

Don't try to put all the checks in the world to your test.

For example: If you need to do a transfer, and we already have a test for transfers, you don't need to check if it
succeeds. Every failed transaction would throw an error and fail the tests, and the exact test for transfers would show
the cause in this case.

Focus on what you need to check. Assume that all the prerequisites for that work as expected. Keep the tests short and
simple.

## Big DONTs

To avoid the test suite becoming a mess, please DON'T:

- Test more than one thing in one `test` block. Tests should be short and check the thing that is written in its
  description.
- Duplicate test logic in multiple tests. If you need to check the same thing twice, try to implement a matcher instead.
- Add tests that don't match suite logic. If your test (or tests) check something that is nor strictly related to the
  topic of suite, create a new suite instead. Suites are being run in parallel, and we should use this fact.
- Perform L1 operations if not strictly required. Depositing and waiting for finalization are operations that take a lot
  of time, especially on stage. It should be avoided, and if it can't be avoided, try to optimize tests so that less
  time is spent waiting without any actions.
- Create accounts yourself. Framework recollects used funds back after the execution, so the only right way to create a
  new wallet is through `TestMaster`.
- Rely on things that you don't control. For example, balance of the fee account: multiple transactions may affect it in
  parallel, so don't expect it to have the exact value.

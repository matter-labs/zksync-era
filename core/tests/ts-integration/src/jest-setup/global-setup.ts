import { TestContextOwner, loadTestEnvironment, waitForServer } from '../index';

declare global {
    var __ZKSYNC_TEST_CONTEXT_OWNER__: TestContextOwner;
}

/**
 * This script performs the initial setup for the integration tests.
 * See `TestContextOwner` class for more details.
 */
async function performSetup(_globalConfig: any, _projectConfig: any) {
    // Perform the test initialization.
    // This is an expensive operation that preceeds running any tests, as we need
    // to deposit & distribute funds, deploy some contracts, and perform basic server checks.

    // Jest writes an initial message without a newline, so we have to do it manually.
    console.log('');

    // Before starting any actual logic, we need to ensure that the server is running (it may not
    // be the case, for example, right after deployment on stage).
    await waitForServer();

    const testEnvironment = await loadTestEnvironment();
    const testContextOwner = new TestContextOwner(testEnvironment);
    const testContext = await testContextOwner.setupContext();

    // Set the test context for test suites to pick up.
    // Currently, jest doesn't provide a way to pass data from `globalSetup` to suites,
    // so we store the data as serialized JSON.
    process.env.ZKSYNC_JEST_TEST_CONTEXT = JSON.stringify(testContext);

    // Store the context object for teardown script, so it can perform, well, the teardown.
    globalThis.__ZKSYNC_TEST_CONTEXT_OWNER__ = testContextOwner;
}

export default performSetup;

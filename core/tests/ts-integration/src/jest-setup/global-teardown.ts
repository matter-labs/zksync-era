import { TestContextOwner } from "../index";

declare global {
  var __ZKSYNC_TEST_CONTEXT_OWNER__: TestContextOwner;
}

/**
 * This script performs the teardown after the whole test suite is completed (either successfully or with some
 * tests failed).
 * It will recollect funds from all the allocated accounts back to the main account they were deposited from.
 */
async function performTeardown(_globalConfig: any, _projectConfig: any) {
  const testContextOwner = globalThis.__ZKSYNC_TEST_CONTEXT_OWNER__;
  await testContextOwner.teardownContext();
}

export default performTeardown;

import { TestMaster } from "../../src";
import * as zksync from "zksync-ethers";
import * as ethers from "ethers";
import fetch from "node-fetch";
import fs from "fs";
import {
  deployContract,
  getContractSource,
  getTestContract,
} from "../../src/helpers";
import { sleep } from "zksync-ethers/build/utils";
import { NodeMode } from "../../src/types";

// Regular expression to match ISO dates.
const DATE_REGEX = /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{6})?/;

const ZKSOLC_VERSION = "v1.5.10";
const SOLC_VERSION = "0.8.26";
const ZK_VM_SOLC_VERSION = "zkVM-0.8.26-1.0.1";

const ZKVYPER_VERSION = "v1.5.4";
const VYPER_VERSION = "0.3.10";

type HttpMethod = "POST" | "GET";

describe("Tests for the contract verification API", () => {
  let testMaster: TestMaster;
  let alice: zksync.Wallet;

  if (process.env.RUN_CONTRACT_VERIFICATION_TEST != "true") {
    test("Contract verification test is not requested to run", () => {
      return;
    });
  } else {
    const contracts = {
      counter: getTestContract("Counter"),
      customAccount: getTestContract("CustomAccount"),
      create: {
        ...getTestContract("Import"),
        factoryDep: getTestContract("Foo").bytecode,
      },
      greeter2: {
        ...getTestContract("Greeter2"),
        factoryDep: getTestContract("Greeter").bytecode,
      },
    };

    beforeAll(() => {
      testMaster = TestMaster.getInstance(__filename);
      alice = testMaster.mainAccount();

      if (testMaster.environment().nodeMode == NodeMode.External) {
        console.warn(
          "You are trying to run contract verification tests on external node. It's not supported.",
        );
      }
    });

    test("should test contract verification", async () => {
      const counterContract = await deployContract(
        alice,
        contracts.counter,
        [],
      );
      const constructorArguments = counterContract.interface.encodeDeploy([]);

      const requestBody = {
        contractAddress: await counterContract.getAddress(),
        contractName: "contracts/counter/counter.sol:Counter",
        sourceCode: getContractSource("counter/counter.sol"),
        compilerZksolcVersion: ZKSOLC_VERSION,
        compilerSolcVersion: ZK_VM_SOLC_VERSION,
        optimizationUsed: true,
        constructorArguments,
        isSystem: true,
      };
      let requestId = await query(
        "POST",
        "/contract_verification",
        undefined,
        requestBody,
      );

      await expectVerifyRequestToSucceed(requestId, requestBody);
    });

    test("should test multi-files contract verification", async () => {
      const contractFactory = new zksync.ContractFactory(
        contracts.create.abi,
        contracts.create.bytecode,
        alice,
      );
      const contractHandle = await contractFactory.deploy({
        customData: {
          factoryDeps: [contracts.create.factoryDep],
        },
      });
      const importContract = await contractHandle.waitForDeployment();
      const standardJsonInput = {
        language: "Solidity",
        sources: {
          "contracts/create/create.sol": {
            content: getContractSource("create/create.sol"),
          },
          "contracts/create/Foo.sol": {
            content: getContractSource("create/Foo.sol"),
          },
        },
        settings: {
          optimizer: { enabled: true },
          isSystem: true,
        },
      };

      const constructorArguments = importContract.interface.encodeDeploy([]);

      const requestBody = {
        contractAddress: await importContract.getAddress(),
        contractName: "contracts/create/create.sol:Import",
        sourceCode: standardJsonInput,
        codeFormat: "solidity-standard-json-input",
        compilerZksolcVersion: ZKSOLC_VERSION,
        compilerSolcVersion: ZK_VM_SOLC_VERSION,
        optimizationUsed: true,
        constructorArguments,
      };
      let requestId = await query(
        "POST",
        "/contract_verification",
        undefined,
        requestBody,
      );

      await expectVerifyRequestToSucceed(requestId, requestBody);
    });

    test("should test yul contract verification", async () => {
      const contractPath = `${
        testMaster.environment().pathToHome
      }/core/tests/ts-integration/contracts/yul/Empty.yul`;
      const sourceCode = fs.readFileSync(contractPath, "utf8");

      const bytecodePath = `${
        testMaster.environment().pathToHome
      }/core/tests/ts-integration/contracts/yul/artifacts/Empty.yul/yul/Empty.yul.zbin`;
      const bytecode = fs.readFileSync(bytecodePath, "utf8");

      const contractFactory = new zksync.ContractFactory([], bytecode, alice);
      const deployTx = await contractFactory.deploy();
      const contractAddress = await (
        await deployTx.waitForDeployment()
      ).getAddress();

      const requestBody = {
        contractAddress,
        contractName: "Empty",
        sourceCode,
        codeFormat: "yul-single-file",
        compilerZksolcVersion: ZKSOLC_VERSION,
        compilerSolcVersion: ZK_VM_SOLC_VERSION,
        optimizationUsed: true,
        constructorArguments: "0x",
        isSystem: true,
      };
      let requestId = await query(
        "POST",
        "/contract_verification",
        undefined,
        requestBody,
      );

      await expectVerifyRequestToSucceed(requestId, requestBody);
    });

    test("should test vyper contract verification", async () => {
      const contractFactory = new zksync.ContractFactory(
        contracts.greeter2.abi,
        contracts.greeter2.bytecode,
        alice,
      );
      const randomAddress = ethers.hexlify(ethers.randomBytes(20));
      const contractHandle = await contractFactory.deploy(randomAddress, {
        customData: {
          factoryDeps: [contracts.greeter2.factoryDep],
        },
      });
      const contract = await contractHandle.waitForDeployment();
      const constructorArguments = contract.interface.encodeDeploy([
        randomAddress,
      ]);

      const requestBody = {
        contractAddress: await contract.getAddress(),
        contractName: "Greeter2",
        sourceCode: {
          Greeter: getContractSource("vyper/Greeter.vy"),
          Greeter2: getContractSource("vyper/Greeter2.vy"),
        },
        codeFormat: "vyper-multi-file",
        compilerZkvyperVersion: ZKVYPER_VERSION,
        compilerVyperVersion: VYPER_VERSION,
        optimizationUsed: true,
        constructorArguments,
      };
      let requestId = await query(
        "POST",
        "/contract_verification",
        undefined,
        requestBody,
      );

      await expectVerifyRequestToSucceed(requestId, requestBody);
    });

    test("Should return zksolc versions", async () => {
      const versions = await query(
        "GET",
        `/contract_verification/zksolc_versions`,
      );
      expect(versions.includes(ZKSOLC_VERSION));
    });

    test("Should return solc versions", async () => {
      const versions = await query(
        "GET",
        `/contract_verification/solc_versions`,
      );
      expect(versions.includes(SOLC_VERSION));
    });

    test("Should return zkvyper versions", async () => {
      const versions = await query(
        "GET",
        `/contract_verification/zkvyper_versions`,
      );
      expect(versions.includes(ZKVYPER_VERSION));
    });

    test("Should return vyper versions", async () => {
      const versions: string[] = await query(
        "GET",
        `/contract_verification/vyper_versions`,
      );
      expect(versions.includes(VYPER_VERSION));
    });

    afterAll(async () => {
      await testMaster.deinitialize();
    });
  }

  /**
   * Performs an API call to the Contract verification API.
   *
   * @param endpoint API endpoint to call.
   * @param queryParams Parameters for a query string.
   * @param requestBody Request body. If provided, a POST request would be met and body would be encoded to JSON.
   * @returns API response parsed as a JSON.
   */
  async function query(
    method: HttpMethod,
    endpoint: string,
    queryParams?: { [key: string]: string },
    requestBody?: any,
  ): Promise<any> {
    const url = new URL(
      endpoint,
      testMaster.environment().contractVerificationUrl,
    );
    // Iterate through query params and add them to URL.
    if (queryParams) {
      Object.entries(queryParams).forEach(([key, value]) =>
        url.searchParams.set(key, value),
      );
    }

    let init = {
      method,
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(requestBody),
    };
    if (requestBody) {
      init.body = JSON.stringify(requestBody);
    }

    let response = await fetch(url, init);
    try {
      return await response.json();
    } catch (e) {
      throw {
        error: "Could not decode JSON in response",
        status: `${response.status} ${response.statusText}`,
      };
    }
  }

  async function expectVerifyRequestToSucceed(
    requestId: number,
    requestBody: any,
  ) {
    let retries = 0;
    while (true) {
      if (retries > 50) {
        throw new Error("Too many retries");
      }

      let statusObject = await query(
        "GET",
        `/contract_verification/${requestId}`,
      );

      if (statusObject.status == "successful") {
        break;
      } else if (statusObject.status == "failed") {
        throw new Error(statusObject.error);
      } else {
        retries += 1;
        await sleep(1000);
      }
    }

    const contract_info = await query(
      "GET",
      `/contract_verification/info/${requestBody.contractAddress}`,
    );

    expect(contract_info).toMatchObject({
      request: {
        id: requestId,
        contractAddress: requestBody.contractAddress.toLowerCase(),
        codeFormat: expect.any(String),
        sourceCode: expect.anything(),
        contractName: requestBody.contractName,
        optimizationUsed: requestBody.optimizationUsed,
        constructorArguments: requestBody.constructorArguments,
        isSystem: expect.any(Boolean),
      },
      artifacts: expect.any(Object),
      verifiedAt: expect.stringMatching(DATE_REGEX),
    });
  }
});

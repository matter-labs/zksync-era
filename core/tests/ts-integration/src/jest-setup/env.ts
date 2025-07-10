import NodeEnvironment from "jest-environment-node";
import type {
  EnvironmentContext,
  JestEnvironmentConfig,
} from "@jest/environment";

export default class IntegrationTestEnvironment extends NodeEnvironment {
  constructor(config: JestEnvironmentConfig, context: EnvironmentContext) {
    super(config, context);
  }

  override async setup() {
    await super.setup();
    // Provide access to raw console in order to produce less cluttered debug messages
    this.global.rawWriteToConsole = console.log;
  }
}

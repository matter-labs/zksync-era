import chalk from "chalk";

const entry = chalk.bold.yellow;
const announce = chalk.yellow;
const success = chalk.green;
const fail = chalk.red;
const timestamp = chalk.grey;
const info = chalk.grey;
const warn = chalk.bgYellow;
const errorPrefix = chalk.bgRed;

/**
 * Represents an action that is currently being performed by the context owner.
 */
interface Action {
  name: string;
  startedAt: Date;
}

/**
 * Status reporter for the framework.
 * Contains utilities for pretty-printing information to the console.
 *
 * Main responsibility is to announce started & finished actions, as well as time required
 * to do so.
 */
export class Reporter {
  /**
   * Stack of actions that are in progress.
   */
  private stack: Action[] = [];

  /**
   * Reports a started action to the console and stores it on the action stack.
   *
   * @param name Name of the action.
   */
  startAction(name: string) {
    const announceLine = this.indent(`${entry(">")} ${announce(name)}`);
    console.log(announceLine);

    this.stack.push({
      name,
      startedAt: new Date(),
    });
  }

  /**
   * Finishes the last action on the stack, reporting it to the console.
   */
  finishAction() {
    const finish = new Date();
    const action = this.pop();

    const time = finish.getTime() - action.startedAt.getTime();
    const successLine = `${success("✔")} ${action.name} done`;
    const timestampLine = timestamp(`(${time}ms)`);
    console.log(this.indent(`${successLine} ${timestampLine}`));
  }

  /**
   * Finishes the last action on the stack, reporting that it failed.
   */
  failAction() {
    const finish = new Date();
    const action = this.pop();

    const time = finish.getTime() - action.startedAt.getTime();
    const failLine = `${fail("❌")} ${action.name} failed`;
    const timestampLine = timestamp(`(${time}ms)`);
    console.log(this.indent(`${failLine} ${timestampLine}`));
  }

  /**
   * Prints a message to the console.
   */
  message(message: string) {
    console.log(this.indent(info(message)));
  }

  /**
   * Prints an easily visible warning to the console.
   */
  warn(message: string) {
    console.log(this.indent(warn(message)));
  }

  /**
   * Prints an error message to the console.
   */
  error(message: string, ...args: any[]) {
    console.log(
      this.indent(`${errorPrefix("Error:")}: ${fail(message)}`),
      ...args,
    );
  }

  /**
   * Prints a debug message.
   * Debug messages are only shown if `ZKSYNC_DEBUG_LOGS` env variable is set.
   */
  debug(message: string, ...args: any[]) {
    if (process.env.ZKSYNC_DEBUG_LOGS) {
      const testName =
        "expect" in globalThis ? expect.getState().currentTestName : undefined;
      // Timestamps only make sense to include in tests.
      const timestampString =
        testName === undefined ? "" : timestamp(`${new Date().toISOString()} `);
      const testString = testName ? info(` [${testName}]`) : "";
      rawWriteToConsole(
        this.indent(`${timestampString}DEBUG${testString}: ${message}`),
        ...args,
      );
    }
  }

  /**
   * Adds required indention to the messages based on the amount of items on the stack.
   */
  private indent(line: string): string {
    return "  ".repeat(this.stack.length) + line;
  }

  /**
   * Pops a message from the stack, throwing an error if the stack is empty.
   */
  private pop(): Action {
    if (this.stack.length === 0) {
      throw new Error("There are no actions on the stack");
    }
    return this.stack.pop()!;
  }
}

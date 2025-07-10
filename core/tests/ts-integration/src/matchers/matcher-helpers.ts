// Common utilities for writing custom matchers.
import { printReceived, printExpected, matcherHint } from "jest-matcher-utils";

// Creates a Jest matcher response for failed test.
export function fail(message: string) {
  return {
    pass: false,
    message: () => message,
  };
}

// Creates a Jest matcher response for succeeded test.
// It still needs a message, as it could've been preceeded by `.not`.
export function pass(message: string) {
  return {
    pass: true,
    message: () => message,
  };
}

export class TestMessage {
  message: string = "";

  // Adds a matcher name hint to the message.
  // Normally should be first method to call.
  matcherHint(name: string): TestMessage {
    this.message += `${matcherHint(name)}\n\n`;
    return this;
  }

  // Adds a line of text to the message
  line(text: string): TestMessage {
    this.message += `${text}\n`;
    return this;
  }

  // Adds a "received" value to the message.
  received(value: any): TestMessage {
    this.message += ` ${printReceived(value)}\n`;
    return this;
  }

  // Adds an "expected" value to the message.
  expected(value: any): TestMessage {
    this.message += ` ${printExpected(value)}\n`;
    return this;
  }

  // Adds an additional information if provided.
  additional(text?: string): TestMessage {
    if (text) {
      this.message += `${text}\n`;
    }
    return this;
  }

  // Returns a built message string.
  build(): string {
    return this.message;
  }
}

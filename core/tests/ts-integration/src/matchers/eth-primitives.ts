import * as ethers from "ethers";
import { TestMessage } from "./matcher-helpers";

// This file contains implementation of matchers for common Ethereum primitives objects.
// For actual doc-comments, see `typings/jest.d.ts` file.

export function toBeAddress(value: string, additionalInfo?: string) {
  const pass = ethers.isAddress(value);

  // Declare messages for normal case and case where matcher was preceded by `.not`.
  let passMessage = new TestMessage()
    .matcherHint(".not.toBeAddress")
    .line("Expected the following string to not be an address:")
    .received(value)
    .additional(additionalInfo)
    .build();

  let failMessage = new TestMessage()
    .matcherHint(".toBeAddress")
    .line("Expected the following string to be an address:")
    .received(value)
    .additional(additionalInfo)
    .build();

  return {
    pass,
    message: () => (pass ? passMessage : failMessage),
  };
}

export function toBeHexString(value: string, additionalInfo?: string) {
  const pass = ethers.isHexString(value);

  // Declare messages for normal case and case where matcher was preceded by `.not`.
  let passMessage = new TestMessage()
    .matcherHint(".not.toBeHexString")
    .line("Expected the following string to not be a hex string:")
    .received(value)
    .additional(additionalInfo)
    .build();

  let failMessage = new TestMessage()
    .matcherHint(".toBeHexString")
    .line("Expected the following string to be a hex string:")
    .received(value)
    .additional(additionalInfo)
    .build();

  return {
    pass,
    message: () => (pass ? passMessage : failMessage),
  };
}

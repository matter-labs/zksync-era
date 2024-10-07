# Generates the list of solidity selectors

This tool generates a mapping from solidity selectors to function names.

The output json file can be used by multiple tools to improve debugging and readability.

By default, it appends the newly found selectors into the list.

To run, first make sure that you have your contracts compiled and then run:

```
cargo run ../../../contracts ../../../etc/selector-generator-data/selectors.json
```

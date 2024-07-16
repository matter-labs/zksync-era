this library contains multiple modules that provide different functionalities for the sequencer/verifier node.


## Modules:
1. **client**: provides communication tools with the Bitcoin network. (broadcast, get block, get transaction confirmation, etc.)
2. **inscriber**: provides tools for creating, signing, and broadcasting inscriptions transactions.
3. **indexer**: provides tools for fetching and parsing Bitcoin blocks. (filter inscriptions transactions, get specific block inscriptions messages, etc.)
4. **transaction_builder**: provides tools for creating unsigned transaction for withdrawal (UTXO selection).
5. **signer**: provides tools for signing transactions.

## responsibilities of shared files:
- **traits.rs**: 
  - contains traits. 
  - these traits should be implemented by modules.
  - some of the modules are dependent to each other, we can use these traits to accept another module instance as a parameter and use its functions.
- **types.rs**:
  - contains types that are shared between modules. (like inscription types, inscription messages, etc.)
  - these types should be used by modules.
  - result and custom errors are defined here.
  - Bitcoin Specific types are defined here.
  - and data structure related to bitcoin like address, private key, etc should have their own type from community standards library.
  - data validation, serialization functions should be implemented here.
- **lib.rs**:
  - contains the public interface of the library.
  - this file should be used for re-exporting the modules.


## Internal Dependencies

- Inscriber depends on Client and Signer modules and should accept them as parameters in the constructor.
  - for broadcasting transactions, fetching UTXOs, setting valid fee, etc Inscriber should use Client module.
  - for signing transactions, Inscriber should use Signer module.
- Indexer depends on Client module and should accept it as a parameter in the constructor.
  - for fetching blocks Indexer should use Client module.
- TransactionBuilder depends on Client module and should accept it as a parameter in the constructor.
  - for fetching UTXOs, setting valid fee, etc TransactionBuilder should use Client module.

## Usage
check [readme.md](./readme.md) for usage examples.

## Testing

unit tests should be implemented for each module in their own file.

for checking the integration and seeing the result of the whole system, we can use the `examples` directory.
this directory is binary and we can import the library and use it in the main function to see the result of the functions.

for running the example, use the following command:

`cargo run --bin via_btc_example`

## Development
before starting implementation of every module, we should define or modify the module's trait in the `traits.rs` file.
and also define or modify the types that are shared between modules in the `types.rs` file.

it's possible that these two file contain trait or type that they are not accurate or needed, don't hesitate to modify or remove them.

write unit tests for each module in their own file.

write integration tests in the `examples` directory.

**Note:**
- only make methods public that are needed by external users.



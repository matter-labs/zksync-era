# Via Network Bitcoin Client Library

This library is responsible for the communication between the sequencer/verifier and the bitcoin network.

This library doesn't contain any logic for the sequencer/verifier, it only provides the communication tools with the
bitcoin network.

**features:**

- Create different type of inscriptions transactions
- Sign inscriptions transactions
- Broadcast inscriptions transactions
- Check for inscriptions transactions confirmation
- Fetch and parse Bitcoin blocks
- Filter Inscriptions transactions from Bitcoin blocks
- Help verifier network participants to create unsigned transaction for withdrawal (UTXO selection)
- Provide helper functions for syncing sequencer/verifier node with the Bitcoin network
  (`indexer::get_inscription_messages`)

## Usage

**Inscribing a message:**

```rust
use via_btc_client;

let bitcoin_inscriber = via_btc_client::BitcoinInscriber::new(
    BTC_RPC_URL
    BTC_PRIVATE_KEY,
    inscription_config,
    client,
    bitcoin_signer
);

let binary_message = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
let inscription_tx = bitcoin_inscriber.create_inscription_tx(binary_message, via_btc_client::InscriptionType::L1BatchCommitment);
let signed_inscription_tx = bitcoin_inscriber.sign_inscription_tx(inscription_tx);
let txid = bitcoin_inscriber.broadcast_inscription_tx(signed_inscription_tx);
let is_confirmed = bitcoin_inscriber.is_tx_confirmed(txid);
```

**Fetching and parsing Bitcoin blocks:**

```rust
use via_btc_client;

let indexer = via_btc_client::Indexer::new(
    BTC_RPC_URL,
    client
);

let block_number = BLOCK_NUMBER;
let inscription_messages : Vec<via_btc_client::InscriptionMessage> = indexer.get_specific_block_inscription_messages(block_number);
```

## Inscription Transaction Flow

```
  1. Unlock all available UTXOs for the source address
  2. Create inscription output with using Taproot approach (stack data):
      - **PUBKEY**
      - OP_CHECKSIG
      - OP_FALSE OP_IF
      - **INSCRIPTION DATA**
      - OP_ENDIF
  3. Create a P2WPKH change output to send the remaining funds back to the source address
  4. Create a transaction with the inputs and outputs
  5. Sign the transaction with the private key
  6. Broadcast the transaction to the network

  PS. Unlock all available UTXO and send the remaining funds back to the source address helps us
      to avoid solving utxo selection problem and we call it the UTXO aggregation approach.
```

# TEE DCAP Collateral Management Specification

## Overview

This document specifies the database design and data access layer for managing Intel DCAP (Data Center Attestation
Primitives) collateral for SGX and TDX trusted execution environments. The collateral data must be periodically updated
on the Ethereum blockchain to ensure TEE attestations remain valid.

## Architecture

### Components

1. **Database Table**: `tee_dcap_collateral` - Stores collateral metadata and Ethereum transaction state
2. **Data Access Layer**: `TeeDcapCollateralDal` - Provides type-safe database operations
3. **Collateral Updater**: Fetches fresh collateral from Intel and generates Ethereum calldata
4. **Ethereum Transaction Manager**: Creates and monitors blockchain transactions

### Data Flow

1. Collateral updater checks expiration dates and fetches new data from Intel
2. New collateral is stored with generated calldata, clearing any existing `eth_tx_id`
3. Transaction manager picks up pending collateral and creates Ethereum transactions
4. Transaction status is monitored with retry capability for failed transactions

## Data Model

### Collateral Types

```rust
#[derive(Debug, Clone)]
pub enum TeeDcapCollateralKind {
    RootCa,              // Intel Root CA certificate
    RootCrl,             // Intel Root CA CRL
    PckCa,               // PCK CA certificate
    PckCrl,              // PCK CA CRL
    SgxTcbInfoJson(FMSPC),     // SGX TCB Info (per FMSPC)
    TdxTcbInfoJson(FMSPC),     // TDX TCB Info (per FMSPC)
    SgxQeIdentityJson,   // SGX Quoting Enclave Identity
    TdxQeIdentityJson,   // TDX Quoting Enclave Identity
}

type FMSPC = [u8; 6];  // Family-Model-Stepping-Platform-CustomSKU
```

### String Representation

Collateral kinds are stored as strings in the database:

- Simple types: `root_ca`, `root_crl`, `pck_ca`, `pck_crl`, `sgx_qe_identity`, `tdx_qe_identity`
- TCB types: `sgx_tcb_{hex_fmspc}`, `tdx_tcb_{hex_fmspc}` (e.g., `sgx_tcb_123456789abc`)

### Database Schema

```sql
CREATE TABLE IF NOT EXISTS tee_dcap_collateral (
    kind                 VARCHAR(20) NOT NULL PRIMARY KEY,  -- Collateral type identifier
    not_after            TIMESTAMPTZ NOT NULL,              -- Expiration timestamp
    sha256               BYTEA       NOT NULL,              -- SHA256 hash of collateral data
    updated              TIMESTAMPTZ NOT NULL,              -- Last update timestamp
    calldata             BYTEA,                             -- Ethereum transaction calldata
    eth_tx_id            INTEGER     REFERENCES eth_txs (id) ON DELETE SET NULL,  -- Link to Ethereum transaction
    update_guard_expires TIMESTAMPTZ                        -- Retry guard expiration
);
```

### Collateral Status

```rust
pub enum TeeDcapCollateralInfo {
    /// The collateral matches what is stored in the database and on chain
    Matches,

    /// The collateral needs updating, recipient must update by the specified time
    UpdateChainBy(DateTime<Utc>),

    /// Another agent is already updating, retry allowed after the specified time
    PendingUpdateBy(DateTime<Utc>),

    /// No record exists, must be created
    RecordMissing,
}
```

## Data Access Layer API

### `TeeDcapCollateralDal` Methods

#### Status Checking

```rust
async fn check_collateral_status(
    &mut self,
    kind: &TeeDcapCollateralKind,
    sha256: &[u8],
) -> DalResult<TeeDcapCollateralInfo>
```

Compares provided collateral against stored data to determine if updates are needed.

#### Data Management

```rust
async fn upsert_collateral(
    &mut self,
    kind: &TeeDcapCollateralKind,
    not_after: DateTime<Utc>,
    sha256: &[u8],
    calldata: Option<&[u8]>,
) -> DalResult<()>
```

Inserts new or updates existing collateral record, clearing any pending transaction.

```rust
async fn get_collateral_by_kind(
    &mut self,
    kind: &TeeDcapCollateralKind,
) -> DalResult<Option<(Vec<u8>, DateTime<Utc>, Option<Vec<u8>>)>>
```

Retrieves specific collateral data: (sha256, not_after, calldata).

#### Transaction Management

```rust
async fn get_pending_collateral_for_eth_tx(
    &mut self,
) -> DalResult<Vec<(TeeDcapCollateralKind, Vec<u8>)>>
```

Returns all collateral records with calldata but no `eth_tx_id`.

```rust
async fn set_eth_tx_id(
    &mut self,
    kind: &TeeDcapCollateralKind,
    eth_tx_id: i32,
    guard_duration_hours: i64,
) -> DalResult<()>
```

Links collateral to an Ethereum transaction with retry guard.

```rust
async fn get_collateral_needing_retry(
    &mut self,
) -> DalResult<Vec<(TeeDcapCollateralKind, i32)>>
```

Finds transactions where guard expired and no successful transaction recorded.

```rust
async fn clear_failed_eth_tx(
    &mut self,
    kind: &TeeDcapCollateralKind,
) -> DalResult<()>
```

Clears failed transaction reference to allow retry.

#### Monitoring

```rust
async fn get_expiring_collateral(
    &mut self,
    hours_before_expiry: i64,
) -> DalResult<Vec<(TeeDcapCollateralKind, DateTime<Utc>)>>
```

Returns collateral approaching expiration within specified hours.

## Transaction Lifecycle

1. **Update Detection**: Collateral updater identifies expiring or changed collateral
2. **Data Storage**: New collateral stored with `upsert_collateral()`, setting:
   - `sha256`: Hash of new collateral data
   - `not_after`: Expiration timestamp from Intel
   - `calldata`: Generated Ethereum transaction data
   - `eth_tx_id`: Set to NULL
3. **Transaction Creation**: Transaction manager:

   - Calls `get_pending_collateral_for_eth_tx()`
   - Creates Ethereum transaction via `eth_sender`
   - Calls `set_eth_tx_id()` with guard duration

4. **Transaction Monitoring**:
   - Success tracked in `eth_txs_history.sent_successfully`
   - Failed transactions identified by `get_collateral_needing_retry()`
   - Retries initiated with `clear_failed_eth_tx()`

## Error Handling

- **Parse Errors**: Invalid collateral kind strings return error from `TryFrom<&str>`
- **Transaction Failures**: Retry mechanism with configurable guard duration
- **Concurrent Updates**: `update_guard_expires` prevents duplicate transactions

## Security Considerations

- Collateral integrity verified via SHA256 hashes
- Transaction retry guards prevent spam
- Foreign key constraint ensures transaction consistency
- All timestamps use UTC to avoid timezone issues

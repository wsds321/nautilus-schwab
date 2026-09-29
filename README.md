# nautilus-schwab

Charles Schwab adapter for [Nautilus Trader](https://nautilustrader.io/).

A Rust-native adapter that connects Nautilus Trader to Charles Schwab's Trader API and Market Data API. Built on top of [`schwab-sdk`](https://crates.io/crates/schwab-sdk) 0.5.

## V1 Implementation Status

V1 implements the core `DataClient` and `ExecutionClient` traits with factory support. The adapter is functional for order management and position reconciliation, with streaming deferred to V1.1.

| Capability | Status | Notes |
|---|---|---|
| US Equities & ETFs | ✅ Implemented | Primary target |
| Market Orders | ✅ Implemented | Via `OrderRequest::buy_market()` / `sell_market()` |
| Limit Orders | ✅ Implemented | Via `OrderRequest::buy_limit()` / `sell_limit()` |
| Stop Orders | ✅ Implemented | Stop-market and stop-limit |
| Order Status Reports | ✅ Implemented | Single + batch via REST API |
| Position Reports | ✅ Implemented | Via accounts API with positions |
| Account Balance | ✅ Implemented | Via accounts API |
| OAuth Token Refresh | ✅ Implemented | Automatic via `SchwabTokenProvider` |
| Historical Bars (REST) | 🚧 Stubbed | Request handler logs intent; response conversion deferred |
| Quote Snapshots (REST) | 🚧 Stubbed | Request handler logs intent; response conversion deferred |
| Real-time Quotes | ❌ V1.1 | WebSocket streamer wiring pending |
| Real-time Bars | ❌ V1.1 | WebSocket streamer wiring pending |
| Fill Reports | ❌ V2 | Requires order activity parsing |
| Options | ❌ V2 | |
| Futures | ❌ V2 | |
| Forex | ❌ V2 | |
| OCO/Bracket Orders | ❌ V2 | |

### Known V1 Limitations

- **Sync order methods**: `submit_order`, `modify_order`, `cancel_order` are synchronous (`?Send`). They validate and build the SDK request but defer actual HTTP submission to the engine's async runtime.
- **No fill reports**: `generate_fill_reports` returns an empty list. Real-time fill tracking requires streamer integration.
- **No real-time streamer wiring**: Subscribe methods track subscription state but don't open WebSocket connections. Quote/bar data is available only via REST snapshots.
- **Historical data stubs**: `request_bars`, `request_quotes`, `request_instruments` log intent but don't yet convert SDK responses to Nautilus types.

## Architecture

```
┌─────────────────────────────────────────────────┐
│           Nautilus Trader Engine                │
│  ┌──────────────┐   ┌────────────────────────┐  │
│  │ SchwabData    │   │ SchwabExecution        │  │
│  │ Client        │   │ Client                 │  │
│  └──────┬───────┘   └──────────┬─────────────┘  │
│         │                       │                │
│  ┌──────▼───────────────────────▼─────────────┐  │
│  │          schwab-sdk 0.5 (Rust crate)        │  │
│  │  accounts · orders · market_data            │  │
│  └──────────────────┬─────────────────────────┘  │
│                     │                             │
│  ┌──────────────────▼─────────────────────────┐  │
│  │   SchwabTokenProvider (refresh-on-demand)   │  │
│  │   Reads from schwab-mcp or env vars         │  │
│  └────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

## Project Structure

```
src/
├── lib.rs                  # Crate root, module declarations, re-exports
├── common/
│   ├── credential.rs       # SchwabCredential with SecretString zeroization
│   ├── symbol.rs           # SchwabSymbol type conversions
│   └── enums.rs            # Shared enum mappings
├── oauth/
│   ├── provider.rs         # SchwabTokenProvider (schwab_sdk::TokenProvider impl)
│   └── flow.rs             # OAuth authorization flow helpers
├── http/
│   ├── client.rs           # SchwabHttpClient wrapping schwab-sdk
│   └── error.rs            # Error type mapping
├── data.rs                 # SchwabDataClient (DataClient trait impl)
├── execution.rs            # SchwabExecutionClient (ExecutionClient trait impl)
├── factories.rs            # DataClientFactory + ExecutionClientFactory impls
└── python/
    └── mod.rs              # PyO3 Python bindings
```

## Prerequisites

- Rust 1.75+
- Python 3.10+ (for Python bindings)
- A Charles Schwab Developer account with API access
- OAuth App Key and App Secret from [Schwab Developer Portal](https://developer.schwab.com/)

## Setup

### Option A: Use Existing schwab-mcp Infrastructure (Recommended)

If you already have [schwab-mcp](https://github.com/satr-trading/schwab-mcp) set up with valid tokens, nautilus-schwab can read credentials directly:

```python
from nautilus_schwab import SchwabCredential

# Reads from ~/.local/share/schwab-mcp/token.yaml and credentials.yaml
credential = SchwabCredential.from_schwab_mcp()
```

Or in Rust:
```rust
let provider = SchwabTokenProvider::from_schwab_mcp()?;
// Reads from ~/.local/share/schwab-mcp/token.yaml and credentials.yaml
```

This integrates with the existing OAuth setup used by finrl-trading and avoids duplicate credential management.

To refresh tokens, use the existing script:
```bash
./finrl-trading/scripts/refresh_schwab_oauth.sh
```

### Option B: Environment Variables

```python
from nautilus_schwab import SchwabCredential

# Reads SCHWAB_APP_KEY, SCHWAB_APP_SECRET, SCHWAB_CALLBACK_URL,
# SCHWAB_ACCESS_TOKEN, SCHWAB_REFRESH_TOKEN
credential = SchwabCredential.from_env()
```

### Option C: Direct Construction

```python
from nautilus_schwab import SchwabCredential

credential = SchwabCredential(
    app_key="your-app-key",
    app_secret="your-app-secret",
    callback_url="https://127.0.0.1/callback",
    access_token="current-access-token",
    refresh_token="current-refresh-token",
)
```

### Build

```bash
# Rust only
cargo build

# With Python bindings
pip install maturin
maturin develop
```

## Python API

### Credential Management

```python
from nautilus_schwab import SchwabCredential

# Three ways to obtain credentials:
cred = SchwabCredential.from_schwab_mcp()  # Recommended
cred = SchwabCredential.from_env()
cred = SchwabCredential(app_key, app_secret, callback_url, access_token, refresh_token)
```

### Configuration

```python
from nautilus_schwab import SchwabDataClientConfig, SchwabExecutionClientConfig

data_config = SchwabDataClientConfig(client_id="SCHWAB")
exec_config = SchwabExecutionClientConfig(
    client_id="SCHWAB",
    account_id="SCHWAB-001",
    default_account=None,  # Optional: specific account number
)
```

### Factories

```python
from nautilus_schwab import SchwabDataClientFactory, SchwabExecutionClientFactory

data_factory = SchwabDataClientFactory()
exec_factory = SchwabExecutionClientFactory()

# Factory metadata
assert data_factory.name == "SCHWAB"
assert data_factory.config_type == "SchwabDataClientConfig"
assert exec_factory.name == "SCHWAB"
assert exec_factory.config_type == "SchwabExecutionClientConfig"
```

### Legacy Functions

```python
from nautilus_schwab import create_data_client, create_execution_client

result = create_data_client(credential, data_config)
result = create_execution_client(credential, exec_config)
```

## Security

- All credentials use `SecretString` with zeroization on drop
- Debug output redacts all sensitive values
- Tokens are never logged or included in error messages
- OAuth tokens stored in memory only (not persisted to disk by this adapter)

## License

MIT

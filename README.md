# nautilus-schwab

Charles Schwab adapter for [Nautilus Trader](https://nautilustrader.io/).

A Rust-native adapter that connects Nautilus Trader to Charles Schwab's Trader API, Market Data API, and real-time WebSocket streamer. Built on top of [`schwab-sdk`](https://crates.io/crates/schwab-sdk) 0.5.

## Features

- **US Equities & ETFs** — full trading support for stocks and exchange-traded funds
- **Order Management** — market, limit, stop-market, and stop-limit orders via REST API
- **Real-time Market Data** — live quotes (`LEVELONE_EQUITIES`) and bars (`CHART_EQUITY`) via WebSocket streamer
- **Account Reconciliation** — position reports, account balances, and order status via REST API
- **OAuth Integration** — automatic token refresh via `SchwabTokenProvider`, compatible with [schwab-mcp](https://github.com/satr-trading/schwab-mcp) infrastructure
- **Python Bindings** — PyO3-based bindings for use with Nautilus Trader's Python API
- **Security-first** — all credentials use `SecretString` with zeroization; tokens never logged

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                  Nautilus Trader Engine                   │
│                                                          │
│  ┌─────────────────┐       ┌──────────────────────────┐  │
│  │ SchwabDataClient │       │ SchwabExecutionClient    │  │
│  │                  │       │                          │  │
│  │ • subscribe_*    │       │ • submit_order           │  │
│  │ • request_*      │       │ • modify_order           │  │
│  │ • streamer task  │       │ • cancel_order           │  │
│  └────────┬─────────┘       │ • generate_*_reports     │  │
│           │                 └────────────┬─────────────┘  │
│           │                              │                │
│  ┌────────▼──────────────────────────────▼─────────────┐  │
│  │              schwab-sdk 0.5 (Rust crate)             │  │
│  │                                                      │  │
│  │  accounts · orders · market_data · streamer          │  │
│  └──────────────────────┬───────────────────────────────┘  │
│                         │                                  │
│  ┌──────────────────────▼───────────────────────────────┐  │
│  │         SchwabTokenProvider (refresh-on-demand)       │  │
│  │         Reads from schwab-mcp or env vars             │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

The data client spawns a background tokio task for the WebSocket streamer. Commands (subscribe, unsubscribe, logout) are sent via an `mpsc` channel from the sync `DataClient` methods to the async streamer processor. Incoming quotes and bars are converted to Nautilus types and dispatched through the message bus.

## Prerequisites

- **Rust 1.89+** (edition 2021)
- **Python 3.10+** (for Python bindings via PyO3 0.29)
- A [Charles Schwab Developer account](https://developer.schwab.com/) with API access
- OAuth App Key and App Secret from the Schwab Developer Portal

## Setup

### 1. Clone and Build

```bash
git clone https://github.com/satr-trading/nautilus-schwab.git
cd nautilus-schwab

# Rust only
cargo build

# With Python bindings
pip install maturin
maturin develop
```

### 2. OAuth Setup

#### Option A: Use Existing schwab-mcp Infrastructure (Recommended)

If you already have [schwab-mcp](https://github.com/satr-trading/schwab-mcp) set up with valid tokens, nautilus-schwab reads credentials directly:

```rust
// Rust
let provider = SchwabTokenProvider::from_schwab_mcp()?;
// Reads from ~/.local/share/schwab-mcp/token.yaml and credentials.yaml
```

```python
# Python
from nautilus_schwab import SchwabCredential
credential = SchwabCredential.from_schwab_mcp()
```

To refresh tokens, use the existing script:
```bash
./finrl-trading/scripts/refresh_schwab_oauth.sh
```

#### Option B: Environment Variables

Set the following environment variables (or put them in a `.env` file):

```bash
SCHWAB_APP_KEY=your-app-key
SCHWAB_APP_SECRET=your-app-secret
SCHWAB_CALLBACK_URL=https://127.0.0.1/callback
SCHWAB_ACCESS_TOKEN=current-access-token
SCHWAB_REFRESH_TOKEN=current-refresh-token
```

```rust
// Rust
let provider = SchwabTokenProvider::from_env()?;
```

```python
# Python
from nautilus_schwab import SchwabCredential
credential = SchwabCredential.from_env()
```

#### Option C: Direct Construction

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

### 3. Running Tests

```bash
# Unit tests (68 tests)
cargo test --lib

# Integration tests (requires SCHWAB_* env vars or schwab-mcp tokens)
cargo test --test integration_test

# All tests
cargo test
```

## Usage

### Rust

```rust
use nautilus_schwab::{SchwabCredential, SchwabTokenProvider};
use nautilus_schwab::data::SchwabDataClient;
use nautilus_schwab::execution::SchwabExecutionClient;

// Create credential and token provider
let credential = SchwabCredential::from_schwab_mcp()?;
let provider = SchwabTokenProvider::new(credential.clone());

// Create clients
let data_client = SchwabDataClient::new(client_id, venue, credential.clone())?;
let exec_client = SchwabExecutionClient::new(client_id, venue, credential)?;

// Connect the streamer for real-time data
data_client.connect_streamer().await?;
```

### Python

```python
from nautilus_schwab import (
    SchwabCredential,
    SchwabDataClientConfig,
    SchwabExecutionClientConfig,
    SchwabDataClientFactory,
    SchwabExecutionClientFactory,
)

# Credential management
cred = SchwabCredential.from_schwab_mcp()  # Recommended
# cred = SchwabCredential.from_env()

# Configuration
data_config = SchwabDataClientConfig(client_id="SCHWAB")
exec_config = SchwabExecutionClientConfig(
    client_id="SCHWAB",
    account_id="SCHWAB-001",
)

# Factories (for Nautilus engine registration)
data_factory = SchwabDataClientFactory()
exec_factory = SchwabExecutionClientFactory()
```

## V1 Capability Matrix

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
| Real-time Quotes | ✅ Implemented | WebSocket streamer (`LEVELONE_EQUITIES`) |
| Real-time Bars | ✅ Implemented | WebSocket streamer (`CHART_EQUITY`) |
| Historical Bars (REST) | 🚧 Stubbed | Request handler logs intent; response conversion deferred |
| Quote Snapshots (REST) | 🚧 Stubbed | Request handler logs intent; response conversion deferred |
| Fill Reports | ❌ V2 | Requires order activity parsing from streamer |
| Options | ❌ V2 | |
| Futures | ❌ V2 | |
| Forex | ❌ V2 | |
| OCO/Bracket Orders | ❌ V2 | |

### Known Limitations

- **Sync order methods**: `submit_order`, `modify_order`, `cancel_order` are synchronous. They validate and build the SDK request but defer actual HTTP submission to the engine's async runtime.
- **No fill reports**: `generate_fill_reports` returns an empty list. Real-time fill tracking requires streamer order-activity parsing (V2).
- **Historical data stubs**: `request_bars`, `request_quotes`, `request_instruments` log intent but don't yet convert SDK responses to Nautilus types.

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
├── data.rs                 # SchwabDataClient (DataClient trait + streamer)
├── execution.rs            # SchwabExecutionClient (ExecutionClient trait)
├── factories.rs            # DataClientFactory + ExecutionClientFactory impls
└── python/
    └── mod.rs              # PyO3 Python bindings

tests/
└── integration_test.rs     # Integration tests (require live credentials)

python/
├── nautilus_schwab/        # Python package stubs
└── nautilus_trader/        # Nautilus adapter registration
```

## Roadmap

### V1.2 — Real Order Execution
- Wire `submit_order` / `modify_order` / `cancel_order` to actual HTTP calls
- Implement fill report generation from streamer order activity
- Complete historical bar and quote snapshot response conversion

### V1.3 — Python Bindings
- Full PyO3 bindings for all client types and configs
- Maturin-based wheel distribution on PyPI
- Integration tests with Nautilus Trader Python engine

### V2.0 — Expanded Asset Classes & Resilience
- Options chain support (quotes, Greeks, multi-leg orders)
- Futures support
- WebSocket auto-reconnect with exponential backoff
- OCO / bracket / trailing-stop orders
- Rate limiting and retry logic

## Security

- All credentials use `SecretString` with zeroization on drop
- Debug output redacts all sensitive values
- Tokens are never logged or included in error messages
- OAuth tokens stored in memory only (not persisted to disk by this adapter)
- `.env` files are gitignored; use `.env.example` as a template

## License

MIT — see [LICENSE](LICENSE).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on reporting bugs, requesting features, and submitting pull requests.

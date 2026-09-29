# nautilus-schwab

Charles Schwab adapter for [Nautilus Trader](https://nautilustrader.io/).

A Rust-native adapter that connects Nautilus Trader to Charles Schwab's Trader API, Market Data API, and real-time WebSocket streamer. Built on top of the [`schwab-sdk`](https://crates.io/crates/schwab-sdk) crate.

## V1 Capability Matrix

| Capability | Status | Notes |
|---|---|---|
| US Equities & ETFs | 🚧 In Progress | Primary target |
| Market Orders | 🚧 In Progress | |
| Limit Orders | 🚧 In Progress | |
| Stop Orders | 🚧 In Progress | |
| Real-time Quotes | 🚧 In Progress | Via WebSocket streamer |
| Real-time Bars | 🚧 In Progress | Via WebSocket streamer |
| Account Balance | 🚧 In Progress | |
| Position Reconciliation | 🚧 In Progress | Startup mass-status |
| OAuth Token Refresh | 🚧 In Progress | Automatic via TokenProvider |
| Options | ❌ V2 | |
| Futures | ❌ V2 | |
| Forex | ❌ V2 | |
| OCO/Bracket Orders | ❌ V2 | |

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
│  │          schwab-sdk (Rust crate)            │  │
│  │  accounts · orders · market_data · streamer │  │
│  └──────────────────┬─────────────────────────┘  │
│                     │                             │
│  ┌──────────────────▼─────────────────────────┐  │
│  │   OAuth TokenProvider (refresh-on-demand)  │  │
│  └────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

## Prerequisites

- Rust 1.75+
- Python 3.10+ (for Python bindings)
- A Charles Schwab Developer account with API access
- OAuth App Key and App Secret from [Schwab Developer Portal](https://developer.schwab.com/)

## Setup

### Option A: Use Existing schwab-mcp Infrastructure (Recommended)

If you already have [schwab-mcp](https://github.com/satr-trading/schwab-mcp) set up with valid tokens, nautilus-schwab can read credentials directly:

```rust
let provider = SchwabTokenProvider::from_schwab_mcp()?;
// Reads from ~/.local/share/schwab-mcp/token.yaml and credentials.yaml
```

This integrates with the existing OAuth setup used by finrl-trading and avoids duplicate credential management.

To refresh tokens, use the existing script:
```bash
./finrl-trading/scripts/refresh_schwab_oauth.sh
```

### Option B: Standalone Setup

1. Register at [developer.schwab.com](https://developer.schwab.com/)
2. Create a new app with "Individual" account type
3. Set callback URL to `https://127.0.0.1/callback`
4. Copy `.env.example` to `.env` and fill in your credentials

### Build

```bash
# Rust only
cargo build

# With Python bindings
pip install maturin
maturin develop
```

## Security

- All credentials use `SecretString` with zeroization on drop
- Debug output redacts all sensitive values
- Tokens are never logged or included in error messages
- OAuth tokens are stored in memory only (not persisted to disk)

## License

MIT

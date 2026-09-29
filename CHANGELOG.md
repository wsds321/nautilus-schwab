# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-09-29

### Added

- **Project skeleton** — Phase 0 Rust crate with Cargo workspace, PyO3 bindings scaffold, and CI-ready structure
- **OAuth integration** — `SchwabTokenProvider` implementing `schwab_sdk::TokenProvider` trait with automatic token refresh; supports reading from schwab-mcp (`~/.local/share/schwab-mcp/token.yaml`), environment variables, or direct construction
- **Credential security** — `SchwabCredential` with `SecretString` zeroization on drop; debug output redacts all sensitive values
- **DataClient** — `SchwabDataClient` implementing `nautilus_common::clients::DataClient` trait with WebSocket streamer integration for real-time quotes (`LEVELONE_EQUITIES`) and bars (`CHART_EQUITY`) via background tokio task
- **ExecutionClient** — `SchwabExecutionClient` implementing `nautilus_common::clients::ExecutionClient` trait with order validation, position reports, account balance, and order status queries via REST API
- **Factories** — `SchwabDataClientFactory` and `SchwabExecutionClientFactory` for Nautilus engine registration
- **Python bindings** — PyO3 module exposing `SchwabCredential`, config types, factory types, and legacy constructor functions
- **Streamer integration** — Background async task processing subscribe/unsubscribe commands via mpsc channel; converts `StreamerResponse` to Nautilus quote and bar types
- **Unit tests** — 68 unit tests covering credential handling, symbol conversion, enum mapping, token provider, HTTP client error mapping, data client streamer logic, execution client order building, and factory metadata
- **Integration tests** — 5 integration tests for Schwab API connectivity (require live credentials)
- **Documentation** — README with architecture diagram, setup instructions, usage examples, and capability matrix

### Notes

- Historical bar and quote snapshot REST handlers are stubbed (log intent only); response conversion deferred to V1.2
- Fill report generation returns empty list; requires streamer order-activity parsing (V2)
- Order submission methods are synchronous; actual HTTP dispatch deferred to engine async runtime (V1.2)

[0.1.0]: https://github.com/satr-trading/nautilus-schwab/releases/tag/v0.1.0

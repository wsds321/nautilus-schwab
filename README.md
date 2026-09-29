# nautilus-schwab

[English](#english) | [中文](#中文) | [日本語](#日本語) | [한국어](#한국어)

---

## English

Charles Schwab adapter for [Nautilus Trader](https://nautilustrader.io/).

A Rust-native adapter that connects Nautilus Trader to Charles Schwab's Trader API, Market Data API, and real-time WebSocket streamer. Built on top of [`schwab-sdk`](https://crates.io/crates/schwab-sdk) 0.5.

### Features

- **US Equities & ETFs** — full trading support for stocks and exchange-traded funds
- **Order Management** — market, limit, stop-market, and stop-limit orders via REST API
- **Real-time Market Data** — live quotes (`LEVELONE_EQUITIES`) and bars (`CHART_EQUITY`) via WebSocket streamer
- **Account Reconciliation** — position reports, account balances, and order status via REST API
- **OAuth Integration** — automatic token refresh via `SchwabTokenProvider`, compatible with [schwab-mcp](https://github.com/satr-trading/schwab-mcp) infrastructure
- **Python Bindings** — PyO3-based bindings for use with Nautilus Trader's Python API
- **Security-first** — all credentials use `SecretString` with zeroization; tokens never logged

### Architecture

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

### Prerequisites

- **Rust 1.89+** (edition 2021)
- **Python 3.10+** (for Python bindings via PyO3 0.29)
- A [Charles Schwab Developer account](https://developer.schwab.com/) with API access
- OAuth App Key and App Secret from the Schwab Developer Portal

### Installation

```bash
pip install nautilus-schwab    # Python users
cargo add nautilus-schwab      # Rust users
```

#### Building from Source

```bash
git clone https://github.com/satr-trading/nautilus-schwab.git
cd nautilus-schwab

# Rust only
cargo build

# With Python bindings
pip install maturin
maturin develop
```

### OAuth Setup

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

### Usage

#### Rust

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

#### Python

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

### V1 Capability Matrix

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

### Project Structure

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

### Roadmap

#### V1.2 — Real Order Execution
- Wire `submit_order` / `modify_order` / `cancel_order` to actual HTTP calls
- Implement fill report generation from streamer order activity
- Complete historical bar and quote snapshot response conversion

#### V1.3 — Python Bindings
- Full PyO3 bindings for all client types and configs
- Maturin-based wheel distribution on PyPI
- Integration tests with Nautilus Trader Python engine

#### V2.0 — Expanded Asset Classes & Resilience
- Options chain support (quotes, Greeks, multi-leg orders)
- Futures support
- WebSocket auto-reconnect with exponential backoff
- OCO / bracket / trailing-stop orders
- Rate limiting and retry logic

### Security

- All credentials use `SecretString` with zeroization on drop
- Debug output redacts all sensitive values
- Tokens are never logged or included in error messages
- OAuth tokens stored in memory only (not persisted to disk by this adapter)
- `.env` files are gitignored; use `.env.example` as a template

### License

MIT — see [LICENSE](LICENSE).

### Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on reporting bugs, requesting features, and submitting pull requests.

---

## 中文

[Nautilus Trader](https://nautilustrader.io/) 的 Charles Schwab 适配器。

一个 Rust 原生适配器，将 Nautilus Trader 连接到 Charles Schwab 的交易 API、行情数据 API 和实时 WebSocket 推送服务。基于 [`schwab-sdk`](https://crates.io/crates/schwab-sdk) 0.5 构建。

### 功能特性

- **美股与 ETF** — 完整的股票和交易所交易基金交易支持
- **订单管理** — 通过 REST API 支持市价单、限价单、止损市价单和止损限价单
- **实时行情数据** — 通过 WebSocket 推送实时报价（`LEVELONE_EQUITIES`）和 K 线（`CHART_EQUITY`）
- **账户对账** — 通过 REST API 获取持仓报告、账户余额和订单状态
- **OAuth 集成** — 通过 `SchwabTokenProvider` 自动刷新令牌，兼容 [schwab-mcp](https://github.com/satr-trading/schwab-mcp) 基础设施
- **Python 绑定** — 基于 PyO3 的绑定，可与 Nautilus Trader 的 Python API 配合使用
- **安全优先** — 所有凭证使用 `SecretString` 并在释放时清零；令牌永远不会被记录到日志

### 架构

```
┌──────────────────────────────────────────────────────────┐
│                  Nautilus Trader 引擎                     │
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
│  │         SchwabTokenProvider (按需刷新令牌)             │  │
│  │         从 schwab-mcp 或环境变量读取                   │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

数据客户端会启动一个后台 tokio 任务来处理 WebSocket 推送。命令（订阅、取消订阅、登出）通过 `mpsc` 通道从同步的 `DataClient` 方法发送到异步的推送处理器。收到的报价和 K 线数据会被转换为 Nautilus 类型并通过消息总线分发。

### 前置要求

- **Rust 1.89+**（edition 2021）
- **Python 3.10+**（用于 PyO3 0.29 的 Python 绑定）
- [Charles Schwab 开发者账户](https://developer.schwab.com/)，已开通 API 访问权限
- 从 Schwab 开发者门户获取的 OAuth App Key 和 App Secret

### 安装

```bash
pip install nautilus-schwab    # Python 用户
cargo add nautilus-schwab      # Rust 用户
```

#### 从源码构建

```bash
git clone https://github.com/satr-trading/nautilus-schwab.git
cd nautilus-schwab

# 仅构建 Rust
cargo build

# 包含 Python 绑定
pip install maturin
maturin develop
```

### OAuth 配置

#### 方式 A：使用现有 schwab-mcp 基础设施（推荐）

如果你已经配置了 [schwab-mcp](https://github.com/satr-trading/schwab-mcp) 并有有效的令牌，nautilus-schwab 可以直接读取凭证：

```rust
// Rust
let provider = SchwabTokenProvider::from_schwab_mcp()?;
// 从 ~/.local/share/schwab-mcp/token.yaml 和 credentials.yaml 读取
```

```python
# Python
from nautilus_schwab import SchwabCredential
credential = SchwabCredential.from_schwab_mcp()
```

刷新令牌可以使用现有脚本：
```bash
./finrl-trading/scripts/refresh_schwab_oauth.sh
```

#### 方式 B：环境变量

设置以下环境变量（或放入 `.env` 文件）：

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

#### 方式 C：直接构造

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

### 使用示例

#### Rust

```rust
use nautilus_schwab::{SchwabCredential, SchwabTokenProvider};
use nautilus_schwab::data::SchwabDataClient;
use nautilus_schwab::execution::SchwabExecutionClient;

// 创建凭证和令牌提供者
let credential = SchwabCredential::from_schwab_mcp()?;
let provider = SchwabTokenProvider::new(credential.clone());

// 创建客户端
let data_client = SchwabDataClient::new(client_id, venue, credential.clone())?;
let exec_client = SchwabExecutionClient::new(client_id, venue, credential)?;

// 连接推送服务获取实时数据
data_client.connect_streamer().await?;
```

#### Python

```python
from nautilus_schwab import (
    SchwabCredential,
    SchwabDataClientConfig,
    SchwabExecutionClientConfig,
    SchwabDataClientFactory,
    SchwabExecutionClientFactory,
)

# 凭证管理
cred = SchwabCredential.from_schwab_mcp()  # 推荐
# cred = SchwabCredential.from_env()

# 配置
data_config = SchwabDataClientConfig(client_id="SCHWAB")
exec_config = SchwabExecutionClientConfig(
    client_id="SCHWAB",
    account_id="SCHWAB-001",
)

# 工厂类（用于注册到 Nautilus 引擎）
data_factory = SchwabDataClientFactory()
exec_factory = SchwabExecutionClientFactory()
```

### V1 功能矩阵

| 功能 | 状态 | 说明 |
|---|---|---|
| 美股与 ETF | ✅ 已实现 | 主要目标 |
| 市价单 | ✅ 已实现 | 通过 `OrderRequest::buy_market()` / `sell_market()` |
| 限价单 | ✅ 已实现 | 通过 `OrderRequest::buy_limit()` / `sell_limit()` |
| 止损单 | ✅ 已实现 | 止损市价单和止损限价单 |
| 订单状态报告 | ✅ 已实现 | 单个 + 批量，通过 REST API |
| 持仓报告 | ✅ 已实现 | 通过 accounts API 含持仓信息 |
| 账户余额 | ✅ 已实现 | 通过 accounts API |
| OAuth 令牌刷新 | ✅ 已实现 | 通过 `SchwabTokenProvider` 自动刷新 |
| 实时报价 | ✅ 已实现 | WebSocket 推送（`LEVELONE_EQUITIES`） |
| 实时 K 线 | ✅ 已实现 | WebSocket 推送（`CHART_EQUITY`） |
| 历史 K 线（REST） | 🚧 桩代码 | 请求处理器已记录意图；响应转换待完成 |
| 报价快照（REST） | 🚧 桩代码 | 请求处理器已记录意图；响应转换待完成 |
| 成交报告 | ❌ V2 | 需要解析推送中的订单活动 |
| 期权 | ❌ V2 | |
| 期货 | ❌ V2 | |
| 外汇 | ❌ V2 | |
| OCO/组合订单 | ❌ V2 | |

### 已知限制

- **同步订单方法**：`submit_order`、`modify_order`、`cancel_order` 是同步方法。它们会验证并构建 SDK 请求，但实际的 HTTP 提交由引擎的异步运行时处理。
- **无成交报告**：`generate_fill_reports` 返回空列表。实时成交跟踪需要解析推送中的订单活动（V2）。
- **历史数据桩代码**：`request_bars`、`request_quotes`、`request_instruments` 会记录意图，但尚未将 SDK 响应转换为 Nautilus 类型。

### 项目结构

```
src/
├── lib.rs                  # Crate 根模块，模块声明，重导出
├── common/
│   ├── credential.rs       # SchwabCredential，使用 SecretString 清零
│   ├── symbol.rs           # SchwabSymbol 类型转换
│   └── enums.rs            # 共享枚举映射
├── oauth/
│   ├── provider.rs         # SchwabTokenProvider（实现 schwab_sdk::TokenProvider）
│   └── flow.rs             # OAuth 授权流程辅助函数
├── http/
│   ├── client.rs           # SchwabHttpClient，封装 schwab-sdk
│   └── error.rs            # 错误类型映射
├── data.rs                 # SchwabDataClient（DataClient trait + 推送服务）
├── execution.rs            # SchwabExecutionClient（ExecutionClient trait）
├── factories.rs            # DataClientFactory + ExecutionClientFactory 实现
└── python/
    └── mod.rs              # PyO3 Python 绑定

tests/
└── integration_test.rs     # 集成测试（需要真实凭证）

python/
├── nautilus_schwab/        # Python 包存根
└── nautilus_trader/        # Nautilus 适配器注册
```

### 路线图

#### V1.2 — 真实订单执行
- 将 `submit_order` / `modify_order` / `cancel_order` 连接到实际 HTTP 调用
- 实现从推送订单活动生成成交报告
- 完成历史 K 线和报价快照的响应转换

#### V1.3 — Python 绑定
- 所有客户端类型和配置的完整 PyO3 绑定
- 基于 Maturin 的 wheel 分发
- 与 Nautilus Trader Python 引擎的集成测试

#### V2.0 — 扩展资产类别与弹性
- 期权链支持（报价、Greeks、多腿订单）
- 期货支持
- WebSocket 自动重连（指数退避）
- OCO / 组合 / 追踪止损订单
- 速率限制和重试逻辑

### 安全性

- 所有凭证使用 `SecretString`，在释放时自动清零
- Debug 输出会隐藏所有敏感值
- 令牌永远不会被记录到日志或包含在错误消息中
- OAuth 令牌仅存储在内存中（本适配器不会将其持久化到磁盘）
- `.env` 文件已被 gitignore；请使用 `.env.example` 作为模板

### 许可证

MIT — 详见 [LICENSE](LICENSE)。

### 贡献

请参阅 [CONTRIBUTING.md](CONTRIBUTING.md) 了解报告 Bug、请求功能和提交 Pull Request 的指南。

---

## 日本語

[Nautilus Trader](https://nautilustrader.io/) 向け Charles Schwab アダプター。

Nautilus Trader を Charles Schwab の Trader API、Market Data API、およびリアルタイム WebSocket ストリーマーに接続する Rust ネイティブのアダプターです。[`schwab-sdk`](https://crates.io/crates/schwab-sdk) 0.5 を基盤に構築されています。

### 機能

- **米国株式・ETF** — 株式および上場投資信託の完全な取引サポート
- **注文管理** — REST API による成行注文、指値注文、逆指値成行注文、逆指値指値注文
- **リアルタイム市場データ** — WebSocket ストリーマーによるリアルタイム気配値（`LEVELONE_EQUITIES`）およびバーデータ（`CHART_EQUITY`）
- **口座照合** — REST API によるポジションレポート、口座残高、注文状況の取得
- **OAuth 連携** — `SchwabTokenProvider` による自動トークン更新、[schwab-mcp](https://github.com/satr-trading/schwab-mcp) インフラとの互換性
- **Python バインディング** — Nautilus Trader の Python API で使用可能な PyO3 ベースのバインディング
- **セキュリティ重視** — すべての認証情報に `SecretString` を使用し、解放時にゼロ化。トークンはログに記録されません

### アーキテクチャ

```
┌──────────────────────────────────────────────────────────┐
│                  Nautilus Trader エンジン                  │
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
│  │      SchwabTokenProvider (オンデマンドリフレッシュ)      │  │
│  │      schwab-mcp または環境変数から読み込み              │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

データクライアントは WebSocket ストリーマー用のバックグラウンド tokio タスクを起動します。コマンド（購読、購読解除、ログアウト）は `mpsc` チャネルを通じて同期 `DataClient` メソッドから非同期ストリーマープロセッサに送信されます。受信した気配値とバーデータは Nautilus 型に変換され、メッセージバスを通じて配信されます。

### 前提条件

- **Rust 1.89+**（edition 2021）
- **Python 3.10+**（PyO3 0.29 による Python バインディング用）
- API アクセス権限を持つ [Charles Schwab 開発者アカウント](https://developer.schwab.com/)
- Schwab Developer Portal から取得した OAuth App Key および App Secret

### インストール

```bash
pip install nautilus-schwab    # Python ユーザー向け
cargo add nautilus-schwab      # Rust ユーザー向け
```

#### ソースからのビルド

```bash
git clone https://github.com/satr-trading/nautilus-schwab.git
cd nautilus-schwab

# Rust のみ
cargo build

# Python バインディング付き
pip install maturin
maturin develop
```

### OAuth 設定

#### 方法 A：既存の schwab-mcp インフラを使用（推奨）

すでに [schwab-mcp](https://github.com/satr-trading/schwab-mcp) が有効なトークンで設定されている場合、nautilus-schwab は認証情報を直接読み取ります：

```rust
// Rust
let provider = SchwabTokenProvider::from_schwab_mcp()?;
// ~/.local/share/schwab-mcp/token.yaml と credentials.yaml から読み込み
```

```python
# Python
from nautilus_schwab import SchwabCredential
credential = SchwabCredential.from_schwab_mcp()
```

トークンの更新には既存のスクリプトを使用できます：
```bash
./finrl-trading/scripts/refresh_schwab_oauth.sh
```

#### 方法 B：環境変数

以下の環境変数を設定します（または `.env` ファイルに記載します）：

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

#### 方法 C：直接構築

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

### 使い方

#### Rust

```rust
use nautilus_schwab::{SchwabCredential, SchwabTokenProvider};
use nautilus_schwab::data::SchwabDataClient;
use nautilus_schwab::execution::SchwabExecutionClient;

// 認証情報とトークンプロバイダーを作成
let credential = SchwabCredential::from_schwab_mcp()?;
let provider = SchwabTokenProvider::new(credential.clone());

// クライアントを作成
let data_client = SchwabDataClient::new(client_id, venue, credential.clone())?;
let exec_client = SchwabExecutionClient::new(client_id, venue, credential)?;

// リアルタイムデータ用にストリーマーを接続
data_client.connect_streamer().await?;
```

#### Python

```python
from nautilus_schwab import (
    SchwabCredential,
    SchwabDataClientConfig,
    SchwabExecutionClientConfig,
    SchwabDataClientFactory,
    SchwabExecutionClientFactory,
)

# 認証情報管理
cred = SchwabCredential.from_schwab_mcp()  # 推奨
# cred = SchwabCredential.from_env()

# 設定
data_config = SchwabDataClientConfig(client_id="SCHWAB")
exec_config = SchwabExecutionClientConfig(
    client_id="SCHWAB",
    account_id="SCHWAB-001",
)

# ファクトリ（Nautilus エンジンへの登録用）
data_factory = SchwabDataClientFactory()
exec_factory = SchwabExecutionClientFactory()
```

### V1 機能マトリックス

| 機能 | ステータス | 備考 |
|---|---|---|
| 米国株式・ETF | ✅ 実装済み | 主要ターゲット |
| 成行注文 | ✅ 実装済み | `OrderRequest::buy_market()` / `sell_market()` 経由 |
| 指値注文 | ✅ 実装済み | `OrderRequest::buy_limit()` / `sell_limit()` 経由 |
| 逆指値注文 | ✅ 実装済み | 逆指値成行および逆指値指値 |
| 注文状況レポート | ✅ 実装済み | REST API による単一＋一括取得 |
| ポジションレポート | ✅ 実装済み | ポジション情報付き accounts API 経由 |
| 口座残高 | ✅ 実装済み | accounts API 経由 |
| OAuth トークン更新 | ✅ 実装済み | `SchwabTokenProvider` による自動更新 |
| リアルタイム気配値 | ✅ 実装済み | WebSocket ストリーマー（`LEVELONE_EQUITIES`） |
| リアルタイムバー | ✅ 実装済み | WebSocket ストリーマー（`CHART_EQUITY`） |
| 履歴バー（REST） | 🚧 スタブ | リクエストハンドラは意図を記録；レスポンス変換は未実装 |
| 気配値スナップショット（REST） | 🚧 スタブ | リクエストハンドラは意図を記録；レスポンス変換は未実装 |
| 約定レポート | ❌ V2 | ストリーマーの注文アクティビティ解析が必要 |
| オプション | ❌ V2 | |
| 先物 | ❌ V2 | |
| 外国為替 | ❌ V2 | |
| OCO/ブラケット注文 | ❌ V2 | |

### 既知の制限事項

- **同期注文メソッド**：`submit_order`、`modify_order`、`cancel_order` は同期メソッドです。SDK リクエストの検証と構築を行いますが、実際の HTTP 送信はエンジンの非同期ランタイムに委譲されます。
- **約定レポートなし**：`generate_fill_reports` は空のリストを返します。リアルタイムの約定追跡にはストリーマーの注文アクティビティ解析が必要です（V2）。
- **履歴データスタブ**：`request_bars`、`request_quotes`、`request_instruments` は意図を記録しますが、SDK レスポンスを Nautilus 型に変換する処理はまだ実装されていません。

### プロジェクト構成

```
src/
├── lib.rs                  # クレートルート、モジュール宣言、再エクスポート
├── common/
│   ├── credential.rs       # SchwabCredential、SecretString によるゼロ化
│   ├── symbol.rs           # SchwabSymbol 型変換
│   └── enums.rs            # 共有列挙型マッピング
├── oauth/
│   ├── provider.rs         # SchwabTokenProvider（schwab_sdk::TokenProvider 実装）
│   └── flow.rs             # OAuth 認可フローヘルパー
├── http/
│   ├── client.rs           # SchwabHttpClient、schwab-sdk のラッパー
│   └── error.rs            # エラー型マッピング
├── data.rs                 # SchwabDataClient（DataClient トレイト + ストリーマー）
├── execution.rs            # SchwabExecutionClient（ExecutionClient トレイト）
├── factories.rs            # DataClientFactory + ExecutionClientFactory 実装
└── python/
    └── mod.rs              # PyO3 Python バインディング

tests/
└── integration_test.rs     # 統合テスト（本番認証情報が必要）

python/
├── nautilus_schwab/        # Python パッケージスタブ
└── nautilus_trader/        # Nautilus アダプター登録
```

### ロードマップ

#### V1.2 — 実際の注文実行
- `submit_order` / `modify_order` / `cancel_order` を実際の HTTP 呼び出しに接続
- ストリーマーの注文アクティビティから約定レポート生成を実装
- 履歴バーおよび気配値スナップショットのレスポンス変換を完了

#### V1.3 — Python バインディング
- すべてのクライアント型と設定に対する完全な PyO3 バインディング
- Maturin ベースの wheel ディストリビューション（PyPI）
- Nautilus Trader Python エンジンとの統合テスト

#### V2.0 — 資産クラスの拡大とレジリエンス
- オプションチェーンサポート（気配値、Greeks、マルチレッグ注文）
- 先物サポート
- 指数バックオフによる WebSocket 自動再接続
- OCO / ブラケット / トレーリングストップ注文
- レート制限とリトライロジック

### セキュリティ

- すべての認証情報に `SecretString` を使用し、ドロップ時にゼロ化
- デバッグ出力はすべての機密値を隠蔽
- トークンはログに記録されず、エラーメッセージにも含まれません
- OAuth トークンはメモリ内にのみ保存（本アダプターはディスクに永続化しません）
- `.env` ファイルは gitignore に登録済み。`.env.example` をテンプレートとして使用してください

### ライセンス

MIT — [LICENSE](LICENSE) を参照してください。

### コントリビューション

バグ報告、機能リクエスト、プルリクエストのガイドラインについては [CONTRIBUTING.md](CONTRIBUTING.md) を参照してください。

---

## 한국어

[Nautilus Trader](https://nautilustrader.io/)용 Charles Schwab 어댑터입니다.

Nautilus Trader를 Charles Schwab의 Trader API, Market Data API 및 실시간 WebSocket 스트리머에 연결하는 Rust 네이티브 어댑터입니다. [`schwab-sdk`](https://crates.io/crates/schwab-sdk) 0.5를 기반으로 구축되었습니다.

### 기능

- **미국 주식 및 ETF** — 주식 및 상장지수펀드에 대한 완전한 거래 지원
- **주문 관리** — REST API를 통한 시장가 주문, 지정가 주문, 손절 시장가 주문, 손절 지정가 주문
- **실시간 시세 데이터** — WebSocket 스트리머를 통한 실시간 호가(`LEVELONE_EQUITIES`) 및 바 데이터(`CHART_EQUITY`)
- **계좌 대조** — REST API를 통한 포지션 보고서, 계좌 잔고 및 주문 상태 조회
- **OAuth 연동** — `SchwabTokenProvider`를 통한 자동 토큰 갱신, [schwab-mcp](https://github.com/satr-trading/schwab-mcp) 인프라와 호환
- **Python 바인딩** — Nautilus Trader의 Python API에서 사용 가능한 PyO3 기반 바인딩
- **보안 우선** — 모든 자격 증명에 `SecretString`을 사용하여 해제 시 제로화되며, 토큰은 로그에 기록되지 않습니다

### 아키텍처

```
┌──────────────────────────────────────────────────────────┐
│                  Nautilus Trader 엔진                     │
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
│  │      SchwabTokenProvider (온디맨드 토큰 갱신)         │  │
│  │      schwab-mcp 또는 환경 변수에서 읽기               │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

데이터 클라이언트는 WebSocket 스트리머를 위한 백그라운드 tokio 태스크를 시작합니다. 명령(구독, 구독 해제, 로그아웃)은 `mpsc` 채널을 통해 동기 `DataClient` 메서드에서 비동기 스트리머 프로세서로 전송됩니다. 수신된 호가와 바 데이터는 Nautilus 타입으로 변환되어 메시지 버스를 통해 전달됩니다.

### 사전 요구사항

- **Rust 1.89+** (edition 2021)
- **Python 3.10+** (PyO3 0.29를 통한 Python 바인딩용)
- API 접근 권한이 있는 [Charles Schwab 개발자 계정](https://developer.schwab.com/)
- Schwab Developer Portal에서 발급받은 OAuth App Key 및 App Secret

### 설치

```bash
pip install nautilus-schwab    # Python 사용자
cargo add nautilus-schwab      # Rust 사용자
```

#### 소스에서 빌드

```bash
git clone https://github.com/satr-trading/nautilus-schwab.git
cd nautilus-schwab

# Rust만 빌드
cargo build

# Python 바인딩 포함
pip install maturin
maturin develop
```

### OAuth 설정

#### 방법 A: 기존 schwab-mcp 인프라 사용 (권장)

이미 [schwab-mcp](https://github.com/satr-trading/schwab-mcp)가 유효한 토큰으로 설정되어 있다면, nautilus-schwab은 자격 증명을 직접 읽을 수 있습니다:

```rust
// Rust
let provider = SchwabTokenProvider::from_schwab_mcp()?;
// ~/.local/share/schwab-mcp/token.yaml 및 credentials.yaml에서 읽기
```

```python
# Python
from nautilus_schwab import SchwabCredential
credential = SchwabCredential.from_schwab_mcp()
```

토큰 갱신에는 기존 스크립트를 사용할 수 있습니다:
```bash
./finrl-trading/scripts/refresh_schwab_oauth.sh
```

#### 방법 B: 환경 변수

다음 환경 변수를 설정합니다 (또는 `.env` 파일에 작성합니다):

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

#### 방법 C: 직접 생성

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

### 사용법

#### Rust

```rust
use nautilus_schwab::{SchwabCredential, SchwabTokenProvider};
use nautilus_schwab::data::SchwabDataClient;
use nautilus_schwab::execution::SchwabExecutionClient;

// 자격 증명 및 토큰 제공자 생성
let credential = SchwabCredential::from_schwab_mcp()?;
let provider = SchwabTokenProvider::new(credential.clone());

// 클라이언트 생성
let data_client = SchwabDataClient::new(client_id, venue, credential.clone())?;
let exec_client = SchwabExecutionClient::new(client_id, venue, credential)?;

// 실시간 데이터를 위해 스트리머 연결
data_client.connect_streamer().await?;
```

#### Python

```python
from nautilus_schwab import (
    SchwabCredential,
    SchwabDataClientConfig,
    SchwabExecutionClientConfig,
    SchwabDataClientFactory,
    SchwabExecutionClientFactory,
)

# 자격 증명 관리
cred = SchwabCredential.from_schwab_mcp()  # 권장
# cred = SchwabCredential.from_env()

# 설정
data_config = SchwabDataClientConfig(client_id="SCHWAB")
exec_config = SchwabExecutionClientConfig(
    client_id="SCHWAB",
    account_id="SCHWAB-001",
)

# 팩토리 (Nautilus 엔진 등록용)
data_factory = SchwabDataClientFactory()
exec_factory = SchwabExecutionClientFactory()
```

### V1 기능 매트릭스

| 기능 | 상태 | 비고 |
|---|---|---|
| 미국 주식 및 ETF | ✅ 구현 완료 | 주요 대상 |
| 시장가 주문 | ✅ 구현 완료 | `OrderRequest::buy_market()` / `sell_market()` 사용 |
| 지정가 주문 | ✅ 구현 완료 | `OrderRequest::buy_limit()` / `sell_limit()` 사용 |
| 손절 주문 | ✅ 구현 완료 | 손절 시장가 및 손절 지정가 |
| 주문 상태 보고서 | ✅ 구현 완료 | REST API를 통한 단일 + 일괄 조회 |
| 포지션 보고서 | ✅ 구현 완료 | 포지션 정보 포함 accounts API 사용 |
| 계좌 잔고 | ✅ 구현 완료 | accounts API 사용 |
| OAuth 토큰 갱신 | ✅ 구현 완료 | `SchwabTokenProvider`를 통한 자동 갱신 |
| 실시간 호가 | ✅ 구현 완료 | WebSocket 스트리머 (`LEVELONE_EQUITIES`) |
| 실시간 바 | ✅ 구현 완료 | WebSocket 스트리머 (`CHART_EQUITY`) |
| 과거 바 (REST) | 🚧 스텁 | 요청 핸들러가 의도를 기록함; 응답 변환은 미구현 |
| 호가 스냅샷 (REST) | 🚧 스텁 | 요청 핸들러가 의도를 기록함; 응답 변환은 미구현 |
| 체결 보고서 | ❌ V2 | 스트리머의 주문 활동 파싱 필요 |
| 옵션 | ❌ V2 | |
| 선물 | ❌ V2 | |
| 외환 | ❌ V2 | |
| OCO/브래킷 주문 | ❌ V2 | |

### 알려진 제한사항

- **동기 주문 메서드**: `submit_order`, `modify_order`, `cancel_order`는 동기 메서드입니다. SDK 요청을 검증하고 구성하지만, 실제 HTTP 전송은 엔진의 비동기 런타임에 위임됩니다.
- **체결 보고서 없음**: `generate_fill_reports`는 빈 리스트를 반환합니다. 실시간 체결 추적에는 스트리머의 주문 활동 파싱이 필요합니다 (V2).
- **과거 데이터 스텁**: `request_bars`, `request_quotes`, `request_instruments`는 의도를 기록하지만, 아직 SDK 응답을 Nautilus 타입으로 변환하지 않습니다.

### 프로젝트 구조

```
src/
├── lib.rs                  # 크레이트 루트, 모듈 선언, 재내보내기
├── common/
│   ├── credential.rs       # SchwabCredential, SecretString 제로화
│   ├── symbol.rs           # SchwabSymbol 타입 변환
│   └── enums.rs            # 공유 열거형 매핑
├── oauth/
│   ├── provider.rs         # SchwabTokenProvider (schwab_sdk::TokenProvider 구현)
│   └── flow.rs             # OAuth 인증 플로우 헬퍼
├── http/
│   ├── client.rs           # SchwabHttpClient, schwab-sdk 래퍼
│   └── error.rs            # 오류 타입 매핑
├── data.rs                 # SchwabDataClient (DataClient 트레잇 + 스트리머)
├── execution.rs            # SchwabExecutionClient (ExecutionClient 트레잇)
├── factories.rs            # DataClientFactory + ExecutionClientFactory 구현
└── python/
    └── mod.rs              # PyO3 Python 바인딩

tests/
└── integration_test.rs     # 통합 테스트 (실제 자격 증명 필요)

python/
├── nautilus_schwab/        # Python 패키지 스텁
└── nautilus_trader/        # Nautilus 어댑터 등록
```

### 로드맵

#### V1.2 — 실제 주문 실행
- `submit_order` / `modify_order` / `cancel_order`를 실제 HTTP 호출에 연결
- 스트리머 주문 활동에서 체결 보고서 생성 구현
- 과거 바 및 호가 스냅샷 응답 변환 완료

#### V1.3 — Python 바인딩
- 모든 클라이언트 타입 및 설정에 대한 완전한 PyO3 바인딩
- Maturin 기반 wheel 배포 (PyPI)
- Nautilus Trader Python 엔진과의 통합 테스트

#### V2.0 — 자산 클래스 확장 및 복원력
- 옵션 체인 지원 (호가, Greeks, 멀티레그 주문)
- 선물 지원
- 지수 백오프를 통한 WebSocket 자동 재연결
- OCO / 브래킷 / 트레일링 스탑 주문
- 속도 제한 및 재시도 로직

### 보안

- 모든 자격 증명에 `SecretString`을 사용하여 드롭 시 제로화
- 디버그 출력은 모든 민감한 값을 숨김 처리
- 토큰은 로그에 기록되거나 오류 메시지에 포함되지 않음
- OAuth 토큰은 메모리에만 저장 (본 어댑터는 디스크에 영구 저장하지 않음)
- `.env` 파일은 gitignore에 등록됨. `.env.example`을 템플릿으로 사용하세요

### 라이선스

MIT — [LICENSE](LICENSE)를 참조하세요.

### 기여하기

버그 신고, 기능 요청 및 Pull Request 제출 가이드라인은 [CONTRIBUTING.md](CONTRIBUTING.md)를 참조하세요.

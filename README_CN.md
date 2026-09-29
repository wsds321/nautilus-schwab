# nautilus-schwab

[Nautilus Trader](https://nautilustrader.io/) 的 Charles Schwab 适配器。

一个 Rust 原生适配器，将 Nautilus Trader 连接到 Charles Schwab 的交易 API、行情数据 API 和实时 WebSocket 推送服务。基于 [`schwab-sdk`](https://crates.io/crates/schwab-sdk) 0.5 构建。

## 功能特性

- **美股与 ETF** — 完整的股票和交易所交易基金交易支持
- **订单管理** — 通过 REST API 支持市价单、限价单、止损市价单和止损限价单
- **实时行情数据** — 通过 WebSocket 推送实时报价（`LEVELONE_EQUITIES`）和 K 线（`CHART_EQUITY`）
- **账户对账** — 通过 REST API 获取持仓报告、账户余额和订单状态
- **OAuth 集成** — 通过 `SchwabTokenProvider` 自动刷新令牌，兼容 [schwab-mcp](https://github.com/satr-trading/schwab-mcp) 基础设施
- **Python 绑定** — 基于 PyO3 的绑定，可与 Nautilus Trader 的 Python API 配合使用
- **安全优先** — 所有凭证使用 `SecretString` 并在释放时清零；令牌永远不会被记录到日志

## 架构

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

## 前置要求

- **Rust 1.89+**（edition 2021）
- **Python 3.10+**（用于 PyO3 0.29 的 Python 绑定）
- [Charles Schwab 开发者账户](https://developer.schwab.com/)，已开通 API 访问权限
- 从 Schwab 开发者门户获取的 OAuth App Key 和 App Secret

## 安装配置

### 1. 克隆并构建

```bash
git clone https://github.com/wsds321/nautilus-schwab.git
cd nautilus-schwab

# 仅构建 Rust
cargo build

# 包含 Python 绑定
pip install maturin
maturin develop
```

### 2. OAuth 配置

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

### 3. 运行测试

```bash
# 单元测试（88 个测试）
cargo test --lib

# 集成测试（需要 SCHWAB_* 环境变量或 schwab-mcp 令牌）
cargo test --test integration_test

# 全部测试
cargo test
```

## 使用示例

### Rust

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

### Python

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

## V1 功能矩阵

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

## 项目结构

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

## 路线图

### V1.2 — 真实订单执行 ✅ 已完成
- 将 `submit_order` / `modify_order` / `cancel_order` 连接到实际 HTTP 调用
- 实现从推送订单活动生成成交报告
- 完成历史 K 线和报价快照的响应转换

### V1.3 — Python 绑定 ✅ 已完成
- 所有客户端类型和配置的完整 PyO3 绑定
- 基于 Maturin 的 wheel 分发
- 与 Nautilus Trader Python 引擎的集成测试

### V2.0 — 扩展资产类别与弹性 ✅ 基础已完成
- 期权链支持（报价、Greeks、多腿订单）
- 期货支持
- WebSocket 自动重连（指数退避）✅ 已实现
- OCO / 组合 / 追踪止损订单
- 速率限制和重试逻辑

## 安全性

- 所有凭证使用 `SecretString`，在释放时自动清零
- Debug 输出会隐藏所有敏感值
- 令牌永远不会被记录到日志或包含在错误消息中
- OAuth 令牌仅存储在内存中（本适配器不会将其持久化到磁盘）
- `.env` 文件已被 gitignore；请使用 `.env.example` 作为模板

## 许可证

MIT — 详见 [LICENSE](LICENSE)。

## 贡献

请参阅 [CONTRIBUTING.md](CONTRIBUTING.md) 了解报告 Bug、请求功能和提交 Pull Request 的指南。

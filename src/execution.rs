//! Schwab ExecutionClient implementation for Nautilus Trader.
//!
//! Implements the `nautilus_common::clients::ExecutionClient` trait, providing:
//! - Order submission via schwab-sdk `OrderRequest` builder
//! - Order cancellation and modification (replace)
//! - Account balance and position queries
//! - Order status report generation
//! - Position status report generation
//!
//! V1 scope: US equities only. Options, futures, forex deferred to V2.

use std::any::Any;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use nautilus_common::clients::ExecutionClient;
use nautilus_common::factories::client::ClientConfig;
use nautilus_common::messages::execution::{
    CancelOrder, GenerateFillReports, GenerateOrderStatusReport, GenerateOrderStatusReports,
    GeneratePositionStatusReports, ModifyOrder, QueryAccount, QueryOrder, SubmitOrder,
};
use nautilus_core::{Params, UnixNanos};
use nautilus_model::accounts::AccountAny;
use nautilus_model::enums::{OmsType, OrderSide, OrderStatus, OrderType, PositionSide, TimeInForce};
use nautilus_model::identifiers::{AccountId, ClientId, InstrumentId, Venue, VenueOrderId};
use nautilus_model::reports::{OrderStatusReport, PositionStatusReport};
use nautilus_model::types::{AccountBalance, MarginBalance, Quantity};
use rust_decimal_macros::dec;
use tracing::{debug, info, warn};

use crate::http::client::SchwabHttpClient;
use crate::oauth::provider::SchwabTokenProvider;
use crate::common::credential::SchwabCredential;

/// Configuration for the Schwab execution client.
#[derive(Debug, Clone)]
pub struct SchwabExecutionClientConfig {
    /// The Nautilus client identifier.
    pub client_id: String,
    /// The Nautilus account identifier.
    pub account_id: String,
    /// Default account number (if multiple accounts exist).
    pub default_account: Option<String>,
}

impl Default for SchwabExecutionClientConfig {
    fn default() -> Self {
        Self {
            client_id: "SCHWAB".to_string(),
            account_id: "SCHWAB-001".to_string(),
            default_account: None,
        }
    }
}

impl ClientConfig for SchwabExecutionClientConfig {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Schwab ExecutionClient for Nautilus Trader.
///
/// Implements the Nautilus `ExecutionClient` trait using schwab-sdk 0.5.
/// All order operations go through the SDK's typestate `OrderRequest` builder
/// and the `orders(account_hash)` namespace.
pub struct SchwabExecutionClient {
    /// HTTP client wrapping schwab-sdk.
    http_client: Arc<SchwabHttpClient>,
    /// Client configuration.
    config: SchwabExecutionClientConfig,
    /// Resolved account hash for API calls (populated on connect/start).
    account_hash: Option<schwab_sdk::AccountHash>,
    /// Whether the client is currently connected.
    is_connected: bool,
    /// Whether stop() has been called (idempotency guard).
    is_stopped: bool,
}

impl SchwabExecutionClient {
    /// Create a new Schwab execution client.
    pub fn new(
        credential: SchwabCredential,
        config: SchwabExecutionClientConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let token_provider = Arc::new(SchwabTokenProvider::new(credential));
        let http_client = Arc::new(SchwabHttpClient::new(token_provider));

        Ok(Self {
            http_client,
            config,
            account_hash: None,
            is_connected: false,
            is_stopped: false,
        })
    }

    /// Resolve the account hash from the configured account number.
    /// If no default account is configured, uses the first available account.
    async fn resolve_account_hash(&self) -> anyhow::Result<schwab_sdk::AccountHash> {
        let numbers = self
            .http_client
            .inner()
            .accounts()
            .numbers()
            .await
            .context("failed to fetch account numbers from Schwab")?;

        if numbers.is_empty() {
            return Err(anyhow!("no Schwab accounts found"));
        }

        // If a default account is specified, find its hash
        if let Some(ref default_acct) = self.config.default_account {
            for entry in &numbers {
                if entry.account_number.expose_secret() == default_acct.as_str() {
                    return Ok(entry.hash_value.clone());
                }
            }
            return Err(anyhow!(
                "configured default account {} not found in Schwab accounts",
                default_acct
            ));
        }

        // Otherwise use the first account
        info!(
            account_number = %numbers[0].account_number.expose_secret(),
            "using first available Schwab account"
        );
        Ok(numbers[0].hash_value.clone())
    }

    /// Get the resolved account hash, returning an error if not yet resolved.
    fn get_account_hash(&self) -> anyhow::Result<&schwab_sdk::AccountHash> {
        self.account_hash
            .as_ref()
            .ok_or_else(|| anyhow!("account hash not resolved; call start() first"))
    }
}

// --- Helper conversions between Nautilus and schwab-sdk types ---

/// Convert a Nautilus OrderSide + OrderType into a schwab-sdk OrderRequest.
fn build_order_request(
    init: &nautilus_model::events::OrderInitialized,
) -> anyhow::Result<schwab_sdk::orders::OrderRequest> {
    let symbol = init.instrument_id.symbol.to_string();
    let qty = init.quantity.as_decimal();

    let builder = match (init.order_side, init.order_type) {
        // Market orders
        (OrderSide::Buy, OrderType::Market) => {
            schwab_sdk::orders::OrderRequest::buy_market(symbol, qty)
        }
        (OrderSide::Sell, OrderType::Market) => {
            schwab_sdk::orders::OrderRequest::sell_market(symbol, qty)
        }

        // Limit orders
        (OrderSide::Buy, OrderType::Limit) => {
            let price = init
                .price
                .map(|p| p.as_decimal())
                .ok_or_else(|| anyhow!("limit buy order requires a price"))?;
            schwab_sdk::orders::OrderRequest::buy_limit(symbol, qty, price)
        }
        (OrderSide::Sell, OrderType::Limit) => {
            let price = init
                .price
                .map(|p| p.as_decimal())
                .ok_or_else(|| anyhow!("limit sell order requires a price"))?;
            schwab_sdk::orders::OrderRequest::sell_limit(symbol, qty, price)
        }

        // Stop orders (stop-market)
        (OrderSide::Buy, OrderType::StopMarket) => {
            let trigger = init
                .trigger_price
                .map(|p| p.as_decimal())
                .ok_or_else(|| anyhow!("stop buy order requires a trigger price"))?;
            // Schwab doesn't have a buy_stop shortcut; use the builder
            schwab_sdk::orders::OrderRequest::single()
                .stop(trigger)
                .equity_buy(symbol, qty)
        }
        (OrderSide::Sell, OrderType::StopMarket) => {
            let trigger = init
                .trigger_price
                .map(|p| p.as_decimal())
                .ok_or_else(|| anyhow!("stop sell order requires a trigger price"))?;
            schwab_sdk::orders::OrderRequest::sell_stop(symbol, qty, trigger)
        }

        // Stop-limit orders
        (OrderSide::Buy, OrderType::StopLimit) => {
            let trigger = init
                .trigger_price
                .map(|p| p.as_decimal())
                .ok_or_else(|| anyhow!("stop-limit buy order requires a trigger price"))?;
            let price = init
                .price
                .map(|p| p.as_decimal())
                .ok_or_else(|| anyhow!("stop-limit buy order requires a limit price"))?;
            schwab_sdk::orders::OrderRequest::single()
                .stop_limit(trigger, price)
                .equity_buy(symbol, qty)
        }
        (OrderSide::Sell, OrderType::StopLimit) => {
            let trigger = init
                .trigger_price
                .map(|p| p.as_decimal())
                .ok_or_else(|| anyhow!("stop-limit sell order requires a trigger price"))?;
            let price = init
                .price
                .map(|p| p.as_decimal())
                .ok_or_else(|| anyhow!("stop-limit sell order requires a limit price"))?;
            schwab_sdk::orders::OrderRequest::sell_stop_limit(symbol, qty, trigger, price)
        }

        (side, otype) => {
            return Err(anyhow!(
                "unsupported order combination: side={:?}, type={:?}",
                side,
                otype
            ));
        }
    };

    // Apply time-in-force
    let duration = match init.time_in_force {
        TimeInForce::Day => schwab_sdk::orders::Duration::Day,
        TimeInForce::Gtc => schwab_sdk::orders::Duration::GoodTillCancel,
        TimeInForce::Fok => schwab_sdk::orders::Duration::FillOrKill,
        TimeInForce::Ioc => schwab_sdk::orders::Duration::ImmediateOrCancel,
        _ => schwab_sdk::orders::Duration::Day, // Default to DAY for unsupported TIFs
    };

    Ok(builder.duration(duration).build())
}

/// Map a schwab-sdk ApiOrderStatus to a Nautilus OrderStatus.
fn map_order_status(status: &schwab_sdk::orders::ApiOrderStatus) -> OrderStatus {
    match status {
        schwab_sdk::orders::ApiOrderStatus::New
        | schwab_sdk::orders::ApiOrderStatus::Accepted => OrderStatus::Accepted,
        schwab_sdk::orders::ApiOrderStatus::Working
        | schwab_sdk::orders::ApiOrderStatus::Queued
        | schwab_sdk::orders::ApiOrderStatus::PendingActivation
        | schwab_sdk::orders::ApiOrderStatus::AwaitingParentOrder
        | schwab_sdk::orders::ApiOrderStatus::AwaitingCondition
        | schwab_sdk::orders::ApiOrderStatus::AwaitingStopCondition
        | schwab_sdk::orders::ApiOrderStatus::AwaitingManualReview
        | schwab_sdk::orders::ApiOrderStatus::AwaitingUrOut
        | schwab_sdk::orders::ApiOrderStatus::AwaitingReleaseTime
        | schwab_sdk::orders::ApiOrderStatus::PendingAcknowledgement => OrderStatus::Accepted,
        schwab_sdk::orders::ApiOrderStatus::Filled => OrderStatus::Filled,
        schwab_sdk::orders::ApiOrderStatus::Canceled => OrderStatus::Canceled,
        schwab_sdk::orders::ApiOrderStatus::Rejected => OrderStatus::Rejected,
        schwab_sdk::orders::ApiOrderStatus::Expired => OrderStatus::Expired,
        schwab_sdk::orders::ApiOrderStatus::PendingCancel => OrderStatus::PendingCancel,
        schwab_sdk::orders::ApiOrderStatus::PendingReplace
        | schwab_sdk::orders::ApiOrderStatus::Replaced => OrderStatus::PendingUpdate,
        schwab_sdk::orders::ApiOrderStatus::PendingRecall => OrderStatus::PendingCancel,
        schwab_sdk::orders::ApiOrderStatus::UnknownSchwab | _ => OrderStatus::Initialized,
    }
}

/// Map a schwab-sdk Instruction to a Nautilus OrderSide.
fn map_instruction_to_side(instruction: &schwab_sdk::orders::Instruction) -> OrderSide {
    match instruction {
        schwab_sdk::orders::Instruction::Buy
        | schwab_sdk::orders::Instruction::BuyToCover
        | schwab_sdk::orders::Instruction::BuyToOpen
        | schwab_sdk::orders::Instruction::BuyToClose => OrderSide::Buy,
        schwab_sdk::orders::Instruction::Sell
        | schwab_sdk::orders::Instruction::SellShort
        | schwab_sdk::orders::Instruction::SellToOpen
        | schwab_sdk::orders::Instruction::SellToClose
        | schwab_sdk::orders::Instruction::SellShortExempt => OrderSide::Sell,
        // Exchange and other rare instructions default to Buy
        _ => OrderSide::Buy,
    }
}

/// Map a schwab-sdk OrderType to a Nautilus OrderType.
fn map_schwab_order_type(order_type: &schwab_sdk::orders::OrderType) -> OrderType {
    match order_type {
        schwab_sdk::orders::OrderType::Market => OrderType::Market,
        schwab_sdk::orders::OrderType::Limit => OrderType::Limit,
        schwab_sdk::orders::OrderType::Stop => OrderType::StopMarket,
        schwab_sdk::orders::OrderType::StopLimit => OrderType::StopLimit,
        schwab_sdk::orders::OrderType::TrailingStop => OrderType::TrailingStopMarket,
        _ => OrderType::Limit, // Fallback
    }
}

/// Map a schwab-sdk Duration to a Nautilus TimeInForce.
fn map_duration_to_tif(duration: &schwab_sdk::orders::Duration) -> TimeInForce {
    match duration {
        schwab_sdk::orders::Duration::Day => TimeInForce::Day,
        schwab_sdk::orders::Duration::GoodTillCancel => TimeInForce::Gtc,
        schwab_sdk::orders::Duration::FillOrKill => TimeInForce::Fok,
        schwab_sdk::orders::Duration::ImmediateOrCancel => TimeInForce::Ioc,
        _ => TimeInForce::Day,
    }
}

/// Convert a chrono DateTime to UnixNanos.
fn datetime_to_unix_nanos(dt: &chrono::DateTime<chrono::Utc>) -> UnixNanos {
    let nanos = dt.timestamp_nanos_opt().unwrap_or(0) as u64;
    UnixNanos::from(nanos)
}

/// Build an OrderStatusReport from a schwab-sdk Order response.
fn build_order_status_report(
    order: &schwab_sdk::orders::Order,
    account_id: AccountId,
    ts_init: UnixNanos,
) -> anyhow::Result<OrderStatusReport> {
    let venue_order_id = order
        .order_id
        .map(|id| VenueOrderId::from(id.to_string()))
        .ok_or_else(|| anyhow!("order missing orderId"))?;

    let leg = order.order_leg_collection.first();
    let instrument_id = leg
        .and_then(|l| l.instrument.as_ref())
        .and_then(|i| i.symbol())
        .map(|sym| InstrumentId::from(format!("{}.SCHWAB", sym).as_str()))
        .unwrap_or_else(|| InstrumentId::from("UNKNOWN.SCHWAB"));

    let order_side = leg
        .and_then(|l| l.instruction.as_ref())
        .map(map_instruction_to_side);

    let order_type = order
        .order_type
        .as_ref()
        .map(map_schwab_order_type)
        .unwrap_or(OrderType::Limit);

    let time_in_force = order
        .duration
        .as_ref()
        .map(map_duration_to_tif)
        .unwrap_or(TimeInForce::Day);

    let order_status = order
        .status
        .as_ref()
        .map(map_order_status)
        .unwrap_or(OrderStatus::Initialized);

    let quantity = order
        .quantity
        .map(|q| Quantity::from(q.to_string().as_str()))
        .unwrap_or_else(|| Quantity::from("0"));

    let filled_qty = order
        .filled_quantity
        .map(|q| Quantity::from(q.to_string().as_str()))
        .unwrap_or_else(|| Quantity::from("0"));

    let ts_accepted = order
        .entered_time
        .as_ref()
        .map(datetime_to_unix_nanos)
        .unwrap_or(ts_init);

    let ts_last = order
        .close_time
        .as_ref()
        .map(datetime_to_unix_nanos)
        .unwrap_or(ts_accepted);

    let mut report = OrderStatusReport::new(
        account_id,
        instrument_id,
        None, // client_order_id — not available from Schwab directly
        venue_order_id,
        order_side,
        order_type,
        time_in_force,
        order_status,
        quantity,
        filled_qty,
        ts_accepted,
        ts_last,
        ts_init,
        None,
    );

    // Set optional fields
    report.price = order.price.map(|p| nautilus_model::types::Price::from(p.to_string().as_str()));
    report.trigger_price = order.stop_price.map(|p| nautilus_model::types::Price::from(p.to_string().as_str()));
    report.cancel_reason = order.status_description.clone();

    Ok(report)
}

// --- ExecutionClient trait implementation ---

#[async_trait(?Send)]
impl ExecutionClient for SchwabExecutionClient {
    fn is_connected(&self) -> bool {
        self.is_connected
    }

    fn client_id(&self) -> ClientId {
        ClientId::from(self.config.client_id.as_str())
    }

    fn account_id(&self) -> AccountId {
        AccountId::from(self.config.account_id.as_str())
    }

    fn venue(&self) -> Venue {
        Venue::from("SCHWAB")
    }

    fn oms_type(&self) -> OmsType {
        OmsType::Netting
    }

    fn get_account(&self) -> Option<AccountAny> {
        // Account state is managed externally via generate_account_state
        None
    }

    fn generate_account_state(
        &self,
        _balances: Vec<AccountBalance>,
        _margins: Vec<MarginBalance>,
        _reported: bool,
        _ts_event: UnixNanos,
        _info: Option<Params>,
    ) -> anyhow::Result<()> {
        debug!("generate_account_state called (delegated to engine)");
        Ok(())
    }

    fn start(&mut self) -> anyhow::Result<()> {
        info!("starting SchwabExecutionClient");
        self.is_stopped = false;
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        if self.is_stopped {
            debug!("stop() called but already stopped (idempotent)");
            return Ok(());
        }
        info!("stopping SchwabExecutionClient");
        self.is_connected = false;
        self.is_stopped = true;
        Ok(())
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        if self.is_connected {
            debug!("already connected");
            return Ok(());
        }

        info!("connecting SchwabExecutionClient");

        // Resolve account hash
        let hash = self.resolve_account_hash().await?;
        info!("resolved Schwab account hash");
        self.account_hash = Some(hash);
        self.is_connected = true;

        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        info!("disconnecting SchwabExecutionClient");
        self.is_connected = false;
        self.account_hash = None;
        Ok(())
    }

    fn submit_order(&self, cmd: SubmitOrder) -> anyhow::Result<()> {
        let hash = self.get_account_hash()?;

        info!(
            instrument = %cmd.instrument_id,
            client_order_id = %cmd.client_order_id,
            "submitting order to Schwab"
        );

        let order_request = build_order_request(&cmd.order_init)
            .context("failed to build OrderRequest from Nautilus order")?;

        debug!(
            order_type = ?cmd.order_init.order_type,
            side = ?cmd.order_init.order_side,
            quantity = %cmd.order_init.quantity,
            "order request built successfully; submitting via Schwab API"
        );

        // Bridge sync → async: the ExecutionClient trait requires sync methods,
        // but schwab-sdk is async. block_in_place + Handle::current().block_on()
        // is safe because Nautilus calls these from its own runtime thread pool.
        let http_client = self.http_client.clone();
        let hash_clone = hash.clone();

        let order_id = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                http_client
                    .inner()
                    .orders(&hash_clone)
                    .place(order_request)
                    .await
            })
        })
        .context("failed to submit order to Schwab API")?;

        info!(
            venue_order_id = %order_id,
            client_order_id = %cmd.client_order_id,
            "order submitted successfully to Schwab"
        );

        Ok(())
    }

    fn modify_order(&self, cmd: ModifyOrder) -> anyhow::Result<()> {
        let hash = self.get_account_hash()?;

        let venue_order_id = cmd
            .venue_order_id
            .as_ref()
            .ok_or_else(|| anyhow!("modify_order requires venue_order_id"))?;

        info!(
            venue_order_id = %venue_order_id,
            client_order_id = %cmd.client_order_id,
            "modifying order on Schwab"
        );

        // Parse the venue order ID as i64 for schwab-sdk OrderId
        let order_id_str = venue_order_id.to_string();
        let order_id: i64 = order_id_str
            .parse()
            .context("venue_order_id must be a numeric Schwab order ID")?;
        let sdk_order_id = schwab_sdk::orders::OrderId::from(order_id);

        // To replace an order, we need to fetch the existing order first to get
        // its side, type, symbol, and TIF — then overlay the modified fields.
        let http_client = self.http_client.clone();
        let hash_clone = hash.clone();

        let existing_order = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                http_client
                    .inner()
                    .orders(&hash_clone)
                    .get(sdk_order_id)
                    .await
            })
        })
        .context("failed to fetch existing order for modification")?;

        // Extract existing order details to build replacement
        let leg = existing_order
            .order_leg_collection
            .first()
            .ok_or_else(|| anyhow!("existing order has no legs"))?;

        let symbol = leg
            .instrument
            .as_ref()
            .and_then(|i| i.symbol())
            .ok_or_else(|| anyhow!("existing order leg has no symbol"))?
            .to_string();

        let instruction = leg
            .instruction
            .as_ref()
            .ok_or_else(|| anyhow!("existing order leg has no instruction"))?;

        let side = map_instruction_to_side(instruction);

        // Use modified quantity if provided, otherwise keep existing
        let qty = cmd
            .quantity
            .map(|q| q.as_decimal())
            .unwrap_or_else(|| {
                existing_order
                    .quantity
                    .unwrap_or(rust_decimal::Decimal::ZERO)
            });

        // Build replacement order based on existing order type with modified fields
        let existing_order_type = existing_order
            .order_type
            .as_ref()
            .map(map_schwab_order_type)
            .unwrap_or(OrderType::Limit);

        let builder = match (side, existing_order_type) {
            (OrderSide::Buy, OrderType::Market) => {
                schwab_sdk::orders::OrderRequest::buy_market(symbol, qty)
            }
            (OrderSide::Sell, OrderType::Market) => {
                schwab_sdk::orders::OrderRequest::sell_market(symbol, qty)
            }
            (OrderSide::Buy, OrderType::Limit) => {
                let price = cmd
                    .price
                    .map(|p| p.as_decimal())
                    .or(existing_order.price)
                    .ok_or_else(|| anyhow!("limit buy requires a price"))?;
                schwab_sdk::orders::OrderRequest::buy_limit(symbol, qty, price)
            }
            (OrderSide::Sell, OrderType::Limit) => {
                let price = cmd
                    .price
                    .map(|p| p.as_decimal())
                    .or(existing_order.price)
                    .ok_or_else(|| anyhow!("limit sell requires a price"))?;
                schwab_sdk::orders::OrderRequest::sell_limit(symbol, qty, price)
            }
            (OrderSide::Buy, OrderType::StopMarket) => {
                let trigger = cmd
                    .trigger_price
                    .map(|p| p.as_decimal())
                    .or(existing_order.stop_price)
                    .ok_or_else(|| anyhow!("stop buy requires a trigger price"))?;
                schwab_sdk::orders::OrderRequest::single()
                    .stop(trigger)
                    .equity_buy(symbol, qty)
            }
            (OrderSide::Sell, OrderType::StopMarket) => {
                let trigger = cmd
                    .trigger_price
                    .map(|p| p.as_decimal())
                    .or(existing_order.stop_price)
                    .ok_or_else(|| anyhow!("stop sell requires a trigger price"))?;
                schwab_sdk::orders::OrderRequest::sell_stop(symbol, qty, trigger)
            }
            (OrderSide::Buy, OrderType::StopLimit) => {
                let trigger = cmd
                    .trigger_price
                    .map(|p| p.as_decimal())
                    .or(existing_order.stop_price)
                    .ok_or_else(|| anyhow!("stop-limit buy requires a trigger price"))?;
                let price = cmd
                    .price
                    .map(|p| p.as_decimal())
                    .or(existing_order.price)
                    .ok_or_else(|| anyhow!("stop-limit buy requires a limit price"))?;
                schwab_sdk::orders::OrderRequest::single()
                    .stop_limit(trigger, price)
                    .equity_buy(symbol, qty)
            }
            (OrderSide::Sell, OrderType::StopLimit) => {
                let trigger = cmd
                    .trigger_price
                    .map(|p| p.as_decimal())
                    .or(existing_order.stop_price)
                    .ok_or_else(|| anyhow!("stop-limit sell requires a trigger price"))?;
                let price = cmd
                    .price
                    .map(|p| p.as_decimal())
                    .or(existing_order.price)
                    .ok_or_else(|| anyhow!("stop-limit sell requires a limit price"))?;
                schwab_sdk::orders::OrderRequest::sell_stop_limit(symbol, qty, trigger, price)
            }
            (s, t) => {
                return Err(anyhow!(
                    "unsupported order combination for modify: side={:?}, type={:?}",
                    s,
                    t
                ));
            }
        };

        // Preserve existing duration (TIF)
        let duration = existing_order
            .duration
            .as_ref()
            .map(|d| match d {
                schwab_sdk::orders::Duration::Day => schwab_sdk::orders::Duration::Day,
                schwab_sdk::orders::Duration::GoodTillCancel => {
                    schwab_sdk::orders::Duration::GoodTillCancel
                }
                schwab_sdk::orders::Duration::FillOrKill => {
                    schwab_sdk::orders::Duration::FillOrKill
                }
                schwab_sdk::orders::Duration::ImmediateOrCancel => {
                    schwab_sdk::orders::Duration::ImmediateOrCancel
                }
                _ => schwab_sdk::orders::Duration::Day,
            })
            .unwrap_or(schwab_sdk::orders::Duration::Day);

        let replacement_request = builder.duration(duration).build();

        debug!(
            order_id = order_id,
            quantity = %qty,
            price = ?cmd.price,
            trigger_price = ?cmd.trigger_price,
            "sending replace order to Schwab API"
        );

        let http_client = self.http_client.clone();
        let hash_clone = hash.clone();

        let new_order_id = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                http_client
                    .inner()
                    .orders(&hash_clone)
                    .replace(sdk_order_id, replacement_request)
                    .await
            })
        })
        .context("failed to replace order on Schwab API")?;

        info!(
            old_venue_order_id = %venue_order_id,
            new_venue_order_id = %new_order_id,
            client_order_id = %cmd.client_order_id,
            "order modified successfully on Schwab"
        );

        Ok(())
    }

    fn cancel_order(&self, cmd: CancelOrder) -> anyhow::Result<()> {
        let hash = self.get_account_hash()?;

        let venue_order_id = cmd
            .venue_order_id
            .as_ref()
            .ok_or_else(|| anyhow!("cancel_order requires venue_order_id"))?;

        info!(
            venue_order_id = %venue_order_id,
            client_order_id = %cmd.client_order_id,
            "cancelling order on Schwab"
        );

        // Parse the venue order ID as i64 for schwab-sdk OrderId
        let order_id_str = venue_order_id.to_string();
        let order_id: i64 = order_id_str
            .parse()
            .context("venue_order_id must be a numeric Schwab order ID")?;
        let sdk_order_id = schwab_sdk::orders::OrderId::from(order_id);

        let http_client = self.http_client.clone();
        let hash_clone = hash.clone();

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                http_client
                    .inner()
                    .orders(&hash_clone)
                    .cancel(sdk_order_id)
                    .await
            })
        })
        .context("failed to cancel order on Schwab API")?;

        info!(
            venue_order_id = %venue_order_id,
            client_order_id = %cmd.client_order_id,
            "order cancelled successfully on Schwab"
        );

        Ok(())
    }

    fn query_account(&self, _cmd: QueryAccount) -> anyhow::Result<()> {
        debug!("query_account called; account state managed via generate_account_state");
        Ok(())
    }

    fn query_order(&self, _cmd: QueryOrder) -> anyhow::Result<()> {
        debug!("query_order not supported in V1");
        Ok(())
    }

    async fn generate_order_status_report(
        &self,
        cmd: &GenerateOrderStatusReport,
    ) -> anyhow::Result<Option<OrderStatusReport>> {
        let hash = self.get_account_hash()?;

        // We need a venue_order_id to look up a specific order
        let venue_order_id = match &cmd.venue_order_id {
            Some(id) => id,
            None => {
                warn!("generate_order_status_report called without venue_order_id");
                return Ok(None);
            }
        };

        let order_id_str = venue_order_id.to_string();
        let order_id: i64 = order_id_str
            .parse()
            .context("venue_order_id must be a numeric Schwab order ID")?;

        let sdk_order_id = schwab_sdk::orders::OrderId::from(order_id);

        info!(order_id = order_id, "fetching order status from Schwab");

        let order = self
            .http_client
            .inner()
            .orders(hash)
            .get(sdk_order_id)
            .await
            .context("failed to fetch order from Schwab")?;

        let report = build_order_status_report(&order, self.account_id(), cmd.ts_init)?;
        Ok(Some(report))
    }

    async fn generate_order_status_reports(
        &self,
        cmd: &GenerateOrderStatusReports,
    ) -> anyhow::Result<Vec<OrderStatusReport>> {
        let hash = self.get_account_hash()?;

        info!(open_only = cmd.open_only, "generating order status reports");

        // Fetch recent orders from Schwab
        // Use a reasonable date range (last 30 days if no start specified)
        let end = chrono::Utc::now();
        let start = cmd
            .start
            .map(|ts| {
                let secs = (ts.as_u64() / 1_000_000_000) as i64;
                chrono::DateTime::from_timestamp(secs, 0).unwrap_or(end - chrono::Duration::days(30))
            })
            .unwrap_or(end - chrono::Duration::days(30));

        let orders = self
            .http_client
            .inner()
            .orders(hash)
            .list(start.into(), end.into())
            .send()
            .await
            .context("failed to list orders from Schwab")?;

        let mut reports = Vec::with_capacity(orders.len());
        for order in &orders {
            match build_order_status_report(order, self.account_id(), cmd.ts_init) {
                Ok(report) => {
                    // Filter by open_only if requested
                    if cmd.open_only && report.order_status.is_closed() {
                        continue;
                    }
                    // Filter by instrument_id if specified
                    if let Some(ref filter_instrument) = cmd.instrument_id {
                        if report.instrument_id != *filter_instrument {
                            continue;
                        }
                    }
                    reports.push(report);
                }
                Err(e) => {
                    warn!(error = %e, "skipping order that could not be converted to report");
                }
            }
        }

        info!(count = reports.len(), "generated order status reports");
        Ok(reports)
    }

    async fn generate_fill_reports(
        &self,
        _cmd: GenerateFillReports,
    ) -> anyhow::Result<Vec<nautilus_model::reports::FillReport>> {
        debug!("generate_fill_reports not fully implemented in V1");
        // Fill reports require parsing order activity collections.
        // Deferred to V2 when we integrate with the streamer for real-time fills.
        Ok(Vec::new())
    }

    async fn generate_position_status_reports(
        &self,
        cmd: &GeneratePositionStatusReports,
    ) -> anyhow::Result<Vec<PositionStatusReport>> {
        let hash = self.get_account_hash()?;

        info!("generating position status reports from Schwab");

        let account = self
            .http_client
            .inner()
            .accounts()
            .get(hash)
            .with_positions()
            .send()
            .await
            .context("failed to fetch account with positions from Schwab")?;

        let empty_positions: Vec<schwab_sdk::accounts::Position> = Vec::new();
        let positions = match &account.securities_account {
            schwab_sdk::accounts::SecuritiesAccount::Margin(acct) => &acct.positions,
            schwab_sdk::accounts::SecuritiesAccount::Cash(acct) => &acct.positions,
            _ => &empty_positions,
        };

        let mut reports = Vec::with_capacity(positions.len());
        for pos in positions {
            let symbol = pos
                .instrument
                .as_ref()
                .and_then(|i| match i {
                    schwab_sdk::accounts::AccountsInstrument::Basic(b) => b.symbol.as_deref(),
                    schwab_sdk::accounts::AccountsInstrument::Option(o) => o.symbol.as_deref(),
                    schwab_sdk::accounts::AccountsInstrument::FixedIncome(f) => f.symbol.as_deref(),
                    _ => None,
                })
                .unwrap_or("UNKNOWN");

            let instrument_id = InstrumentId::from(format!("{}.SCHWAB", symbol).as_str());

            // Filter by instrument_id if specified
            if let Some(ref filter_instrument) = cmd.instrument_id {
                if instrument_id != *filter_instrument {
                    continue;
                }
            }

            // Determine position side and quantity
            let long_qty = pos.long_quantity.unwrap_or(dec!(0));
            let short_qty = pos.short_quantity.unwrap_or(dec!(0));

            let (position_side, quantity) = if long_qty > dec!(0) {
                (PositionSide::Long, long_qty)
            } else if short_qty > dec!(0) {
                (PositionSide::Short, short_qty)
            } else {
                continue; // Skip flat positions
            };

            let avg_price = pos.average_price;

            let report = PositionStatusReport::new(
                self.account_id(),
                instrument_id,
                position_side,
                Quantity::from(quantity.to_string().as_str()),
                cmd.ts_init,
                cmd.ts_init,
                None,
                None,
                avg_price,
            );

            reports.push(report);
        }

        info!(count = reports.len(), "generated position status reports");
        Ok(reports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Config defaults ─────────────────────────────────────────────────

    #[test]
    fn test_execution_config_defaults() {
        let config = SchwabExecutionClientConfig::default();
        assert_eq!(config.client_id, "SCHWAB");
        assert_eq!(config.account_id, "SCHWAB-001");
        assert_eq!(config.default_account, None);
    }

    #[test]
    fn test_execution_config_custom() {
        let config = SchwabExecutionClientConfig {
            client_id: "MY-SCHWAB".to_string(),
            account_id: "SCHWAB-002".to_string(),
            default_account: Some("12345678".to_string()),
        };
        assert_eq!(config.client_id, "MY-SCHWAB");
        assert_eq!(config.account_id, "SCHWAB-002");
        assert_eq!(config.default_account.as_deref(), Some("12345678"));
    }

    // ── ClientConfig trait ──────────────────────────────────────────────

    #[test]
    fn test_client_config_trait() {
        let config = SchwabExecutionClientConfig::default();
        // Verify as_any returns the correct type
        let any = config.as_any();
        assert!(any.downcast_ref::<SchwabExecutionClientConfig>().is_some());
    }

    // ── map_order_status ────────────────────────────────────────────────

    #[test]
    fn test_map_order_status_filled() {
        assert_eq!(
            map_order_status(&schwab_sdk::orders::ApiOrderStatus::Filled),
            OrderStatus::Filled
        );
    }

    #[test]
    fn test_map_order_status_canceled() {
        assert_eq!(
            map_order_status(&schwab_sdk::orders::ApiOrderStatus::Canceled),
            OrderStatus::Canceled
        );
    }

    #[test]
    fn test_map_order_status_rejected() {
        assert_eq!(
            map_order_status(&schwab_sdk::orders::ApiOrderStatus::Rejected),
            OrderStatus::Rejected
        );
    }

    #[test]
    fn test_map_order_status_expired() {
        assert_eq!(
            map_order_status(&schwab_sdk::orders::ApiOrderStatus::Expired),
            OrderStatus::Expired
        );
    }

    #[test]
    fn test_map_order_status_accepted_variants() {
        // All these should map to Accepted
        let accepted_statuses = [
            schwab_sdk::orders::ApiOrderStatus::New,
            schwab_sdk::orders::ApiOrderStatus::Accepted,
            schwab_sdk::orders::ApiOrderStatus::Working,
            schwab_sdk::orders::ApiOrderStatus::Queued,
            schwab_sdk::orders::ApiOrderStatus::PendingActivation,
            schwab_sdk::orders::ApiOrderStatus::AwaitingParentOrder,
            schwab_sdk::orders::ApiOrderStatus::AwaitingCondition,
            schwab_sdk::orders::ApiOrderStatus::AwaitingStopCondition,
            schwab_sdk::orders::ApiOrderStatus::AwaitingManualReview,
            schwab_sdk::orders::ApiOrderStatus::AwaitingUrOut,
            schwab_sdk::orders::ApiOrderStatus::AwaitingReleaseTime,
            schwab_sdk::orders::ApiOrderStatus::PendingAcknowledgement,
        ];
        for status in &accepted_statuses {
            assert_eq!(map_order_status(status), OrderStatus::Accepted, "failed for {:?}", status);
        }
    }

    #[test]
    fn test_map_order_status_pending_cancel() {
        assert_eq!(
            map_order_status(&schwab_sdk::orders::ApiOrderStatus::PendingCancel),
            OrderStatus::PendingCancel
        );
    }

    #[test]
    fn test_map_order_status_pending_update() {
        assert_eq!(
            map_order_status(&schwab_sdk::orders::ApiOrderStatus::PendingReplace),
            OrderStatus::PendingUpdate
        );
        assert_eq!(
            map_order_status(&schwab_sdk::orders::ApiOrderStatus::Replaced),
            OrderStatus::PendingUpdate
        );
    }

    #[test]
    fn test_map_order_status_unknown() {
        assert_eq!(
            map_order_status(&schwab_sdk::orders::ApiOrderStatus::UnknownSchwab),
            OrderStatus::Initialized
        );
    }

    // ── map_instruction_to_side ─────────────────────────────────────────

    #[test]
    fn test_map_instruction_buy_variants() {
        let buy_instructions = [
            schwab_sdk::orders::Instruction::Buy,
            schwab_sdk::orders::Instruction::BuyToCover,
            schwab_sdk::orders::Instruction::BuyToOpen,
            schwab_sdk::orders::Instruction::BuyToClose,
        ];
        for instr in &buy_instructions {
            assert_eq!(map_instruction_to_side(instr), OrderSide::Buy, "failed for {:?}", instr);
        }
    }

    #[test]
    fn test_map_instruction_sell_variants() {
        let sell_instructions = [
            schwab_sdk::orders::Instruction::Sell,
            schwab_sdk::orders::Instruction::SellShort,
            schwab_sdk::orders::Instruction::SellToOpen,
            schwab_sdk::orders::Instruction::SellToClose,
            schwab_sdk::orders::Instruction::SellShortExempt,
        ];
        for instr in &sell_instructions {
            assert_eq!(map_instruction_to_side(instr), OrderSide::Sell, "failed for {:?}", instr);
        }
    }

    // ── map_schwab_order_type ───────────────────────────────────────────

    #[test]
    fn test_map_schwab_order_type() {
        assert_eq!(
            map_schwab_order_type(&schwab_sdk::orders::OrderType::Market),
            OrderType::Market
        );
        assert_eq!(
            map_schwab_order_type(&schwab_sdk::orders::OrderType::Limit),
            OrderType::Limit
        );
        assert_eq!(
            map_schwab_order_type(&schwab_sdk::orders::OrderType::Stop),
            OrderType::StopMarket
        );
        assert_eq!(
            map_schwab_order_type(&schwab_sdk::orders::OrderType::StopLimit),
            OrderType::StopLimit
        );
        assert_eq!(
            map_schwab_order_type(&schwab_sdk::orders::OrderType::TrailingStop),
            OrderType::TrailingStopMarket
        );
    }

    // ── map_duration_to_tif ─────────────────────────────────────────────

    #[test]
    fn test_map_duration_to_tif() {
        assert_eq!(
            map_duration_to_tif(&schwab_sdk::orders::Duration::Day),
            TimeInForce::Day
        );
        assert_eq!(
            map_duration_to_tif(&schwab_sdk::orders::Duration::GoodTillCancel),
            TimeInForce::Gtc
        );
        assert_eq!(
            map_duration_to_tif(&schwab_sdk::orders::Duration::FillOrKill),
            TimeInForce::Fok
        );
        assert_eq!(
            map_duration_to_tif(&schwab_sdk::orders::Duration::ImmediateOrCancel),
            TimeInForce::Ioc
        );
    }

    // ── datetime_to_unix_nanos ──────────────────────────────────────────

    #[test]
    fn test_datetime_to_unix_nanos() {
        let dt = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let nanos = datetime_to_unix_nanos(&dt);
        assert_eq!(nanos.as_u64(), 1_700_000_000_000_000_000u64);
    }

    #[test]
    fn test_datetime_to_unix_nanos_epoch() {
        let dt = chrono::DateTime::from_timestamp(0, 0).unwrap();
        let nanos = datetime_to_unix_nanos(&dt);
        assert_eq!(nanos.as_u64(), 0);
    }

    // ── ExecutionClient trait compilation check ─────────────────────────

    #[test]
    fn test_execution_client_trait_compiles() {
        fn _assert_execution_client<T: ExecutionClient>() {}
        _assert_execution_client::<SchwabExecutionClient>();
    }
}

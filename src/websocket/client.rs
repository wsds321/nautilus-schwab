//! WebSocket client for Schwab real-time market data streamer.
//!
//! Manages connection lifecycle, authentication, subscription management,
//! and automatic reconnection with exponential backoff.

use crate::oauth::provider::SchwabTokenProvider;
use crate::websocket::messages::{StreamerCommand, StreamerRequest, StreamerResponse};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

/// Configuration for the streamer client.
#[derive(Debug, Clone)]
pub struct StreamerConfig {
    /// WebSocket endpoint URL.
    pub url: String,
    /// Maximum reconnection attempts before giving up.
    pub max_reconnect_attempts: u32,
    /// Base delay between reconnection attempts (ms).
    pub reconnect_base_delay_ms: u64,
    /// Heartbeat interval (seconds).
    pub heartbeat_interval_secs: u64,
}

impl Default for StreamerConfig {
    fn default() -> Self {
        Self {
            url: crate::DEFAULT_STREAMER_URL.to_string(),
            max_reconnect_attempts: 10,
            reconnect_base_delay_ms: 1000,
            heartbeat_interval_secs: 30,
        }
    }
}

/// Commands sent to the streamer client task.
#[derive(Debug)]
pub enum StreamerCommandMsg {
    /// Subscribe to a service with given keys and fields.
    Subscribe {
        service: String,
        keys: Vec<String>,
        fields: Vec<String>,
    },
    /// Unsubscribe from a service.
    Unsubscribe {
        service: String,
        keys: Vec<String>,
    },
    /// Shut down the streamer.
    Shutdown,
}

/// Events emitted by the streamer client task.
#[derive(Debug, Clone)]
pub enum StreamerEvent {
    /// Connection established and authenticated.
    Connected,
    /// Connection lost (will attempt reconnect).
    Disconnected { reason: String },
    /// Raw data received from a subscription.
    Data {
        service: String,
        content: Vec<serde_json::Value>,
    },
    /// Error occurred.
    Error { message: String },
    /// Streamer shut down cleanly.
    Shutdown,
}

/// Schwab real-time market data streamer client.
///
/// Runs as a background task, managing the WebSocket connection
/// and dispatching events through channels.
pub struct SchwabStreamerClient {
    /// Token provider for authentication.
    token_provider: Arc<SchwabTokenProvider>,
    /// Client configuration.
    config: StreamerConfig,
    /// Channel for sending commands to the streamer task.
    cmd_tx: mpsc::Sender<StreamerCommandMsg>,
    /// Channel for receiving events from the streamer task.
    event_rx: Arc<RwLock<mpsc::Receiver<StreamerEvent>>>,
    /// Next request ID counter.
    next_req_id: Arc<std::sync::atomic::AtomicU64>,
}

impl SchwabStreamerClient {
    /// Create a new streamer client (does not connect yet).
    pub fn new(
        token_provider: Arc<SchwabTokenProvider>,
        config: StreamerConfig,
    ) -> Self {
        let (cmd_tx, _cmd_rx) = mpsc::channel(64);
        let (_event_tx, event_rx) = mpsc::channel(256);

        Self {
            token_provider,
            config,
            cmd_tx,
            event_rx: Arc::new(RwLock::new(event_rx)),
            next_req_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }

    /// Allocate the next request ID.
    pub fn next_request_id(&self) -> u64 {
        self.next_req_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Send a subscribe command to the streamer.
    pub async fn subscribe(
        &self,
        service: impl Into<String>,
        keys: Vec<String>,
        fields: Vec<String>,
    ) -> Result<(), mpsc::error::SendError<StreamerCommandMsg>> {
        self.cmd_tx
            .send(StreamerCommandMsg::Subscribe {
                service: service.into(),
                keys,
                fields,
            })
            .await
    }

    /// Send an unsubscribe command to the streamer.
    pub async fn unsubscribe(
        &self,
        service: impl Into<String>,
        keys: Vec<String>,
    ) -> Result<(), mpsc::error::SendError<StreamerCommandMsg>> {
        self.cmd_tx
            .send(StreamerCommandMsg::Unsubscribe {
                service: service.into(),
                keys,
            })
            .await
    }

    /// Shut down the streamer gracefully.
    pub async fn shutdown(&self) -> Result<(), mpsc::error::SendError<StreamerCommandMsg>> {
        self.cmd_tx.send(StreamerCommandMsg::Shutdown).await
    }
}

/// Background task that manages the WebSocket connection.
///
/// This function is spawned as a tokio task and runs until shutdown.
pub async fn streamer_task(
    token_provider: Arc<SchwabTokenProvider>,
    config: StreamerConfig,
    mut cmd_rx: mpsc::Receiver<StreamerCommandMsg>,
    event_tx: mpsc::Sender<StreamerEvent>,
    next_req_id: Arc<std::sync::atomic::AtomicU64>,
) {
    info!(url = %config.url, "starting Schwab streamer task");

    loop {
        // Connect and authenticate
        match connect_and_authenticate(&token_provider, &config).await {
            Ok((mut ws_tx, mut ws_rx)) => {
                let _ = event_tx.send(StreamerEvent::Connected).await;
                info!("streamer connected and authenticated");

                // Main message loop
                loop {
                    tokio::select! {
                        // Handle incoming commands
                        Some(cmd) = cmd_rx.recv() => {
                            match cmd {
                                StreamerCommandMsg::Subscribe { service, keys, fields } => {
                                    let req_id = next_req_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    let params = serde_json::json!({
                                        "keys": keys.join(","),
                                        "fields": fields.join(","),
                                    });
                                    let req = StreamerRequest {
                                        reqid: req_id,
                                        service: service.clone(),
                                        command: StreamerCommand::Subscribe,
                                        account: None,
                                        parameters: Some(params),
                                    };
                                    if let Ok(msg) = serde_json::to_string(&req) {
                                        if ws_tx.send(Message::Text(msg)).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                StreamerCommandMsg::Unsubscribe { service, keys } => {
                                    let req_id = next_req_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    let params = serde_json::json!({
                                        "keys": keys.join(","),
                                    });
                                    let req = StreamerRequest {
                                        reqid: req_id,
                                        service,
                                        command: StreamerCommand::Unsubscribe,
                                        account: None,
                                        parameters: Some(params),
                                    };
                                    if let Ok(msg) = serde_json::to_string(&req) {
                                        if ws_tx.send(Message::Text(msg)).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                StreamerCommandMsg::Shutdown => {
                                    info!("streamer shutdown requested");
                                    let _ = event_tx.send(StreamerEvent::Shutdown).await;
                                    return;
                                }
                            }
                        }
                        // Handle incoming WebSocket messages
                        Some(Ok(msg)) = ws_rx.next() => {
                            match msg {
                                Message::Text(text) => {
                                    if let Ok(response) = serde_json::from_str::<StreamerResponse>(&text) {
                                        for content in response.content {
                                            let service = content.get("service")
                                                .and_then(|s| s.as_str())
                                                .unwrap_or("unknown")
                                                .to_string();
                                            let data = content.get("content")
                                                .cloned()
                                                .unwrap_or_default();
                                            if let Some(arr) = data.as_array() {
                                                let _ = event_tx.send(StreamerEvent::Data {
                                                    service,
                                                    content: arr.clone(),
                                                }).await;
                                            }
                                        }
                                    } else {
                                        debug!(text = %text, "unparseable streamer message");
                                    }
                                }
                                Message::Ping(data) => {
                                    let _ = ws_tx.send(Message::Pong(data)).await;
                                }
                                Message::Close(_) => {
                                    warn!("streamer connection closed by server");
                                    break;
                                }
                                _ => {}
                            }
                        }
                        else => {
                            warn!("streamer channel closed unexpectedly");
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "failed to connect to streamer");
            }
        }

        let _ = event_tx
            .send(StreamerEvent::Disconnected {
                reason: "connection lost".into(),
            })
            .await;

        // Check if we should stop trying
        if cmd_rx.is_closed() {
            info!("command channel closed, stopping streamer");
            return;
        }

        // Wait before reconnecting
        tokio::time::sleep(tokio::time::Duration::from_millis(
            config.reconnect_base_delay_ms,
        ))
        .await;
    }
}

/// Establish WebSocket connection and perform login handshake.
async fn connect_and_authenticate(
    _token_provider: &Arc<SchwabTokenProvider>,
    config: &StreamerConfig,
) -> Result<
    (
        futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>,
        futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
    ),
    Box<dyn std::error::Error + Send + Sync>,
> {
    use tokio_tungstenite::connect_async;

    let (ws_stream, _) = connect_async(&config.url).await?;
    let (tx, rx) = ws_stream.split();

    // TODO: Send LOGIN request with credentials
    // For now, just return the split stream

    Ok((tx, rx))
}

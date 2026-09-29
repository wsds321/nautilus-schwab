"""Charles Schwab adapter for Nautilus Trader.

Provides Python bindings for the Rust-native Schwab adapter, enabling
integration with Nautilus Trader's Python-based strategy and configuration layer.

Exposed types:
    - ``SchwabCredential`` — OAuth credentials (from env, files, or explicit)
    - ``SchwabDataClientConfig`` — Configuration for the data client
    - ``SchwabExecutionClientConfig`` — Configuration for the execution client
    - ``SchwabDataClientFactory`` — Factory for creating data clients
    - ``SchwabExecutionClientFactory`` — Factory for creating execution clients
    - ``create_data_client()`` — Legacy convenience function
    - ``create_execution_client()`` — Legacy convenience function

Example::

    from nautilus_schwab import SchwabCredential, SchwabDataClientConfig

    cred = SchwabCredential.from_env()
    config = SchwabDataClientConfig(client_id="SCHWAB")
"""

from nautilus_schwab._internal import (
    SchwabCredential,
    SchwabDataClientConfig,
    SchwabDataClientFactory,
    SchwabExecutionClientConfig,
    SchwabExecutionClientFactory,
    create_data_client,
    create_execution_client,
)

__all__ = [
    "SchwabCredential",
    "SchwabDataClientConfig",
    "SchwabDataClientFactory",
    "SchwabExecutionClientConfig",
    "SchwabExecutionClientFactory",
    "create_data_client",
    "create_execution_client",
]

__version__ = "0.1.0"

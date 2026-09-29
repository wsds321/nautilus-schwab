//! PyO3 Python bindings for the Schwab adapter.
//!
//! Exposes Rust types to Python for use within Nautilus Trader's
//! Python-based strategy and configuration layer.
//!
//! ## Exposed Types
//!
//! - `SchwabCredential` — OAuth credentials (from env, files, or explicit)
//! - `SchwabDataClientConfig` — Configuration for the data client
//! - `SchwabExecutionClientConfig` — Configuration for the execution client
//! - `SchwabDataClientFactory` — Factory for creating data clients (Nautilus plugin interface)
//! - `SchwabExecutionClientFactory` — Factory for creating execution clients (Nautilus plugin interface)
//! - `create_data_client()` — Legacy convenience function
//! - `create_execution_client()` — Legacy convenience function

use pyo3::prelude::*;

/// Python module initialization.
///
/// The module name must match the lib name in Cargo.toml (`nautilus_schwab`)
/// and the maturin `module-name` setting (`nautilus_schwab._internal`).
#[pymodule]
fn nautilus_schwab(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Register credential types
    m.add_class::<PySchwabCredential>()?;

    // Register config types
    m.add_class::<PySchwabDataClientConfig>()?;
    m.add_class::<PySchwabExecutionClientConfig>()?;

    // Register factory types
    m.add_class::<PySchwabDataClientFactory>()?;
    m.add_class::<PySchwabExecutionClientFactory>()?;

    // Register legacy factory functions
    m.add_function(wrap_pyfunction!(create_data_client, m)?)?;
    m.add_function(wrap_pyfunction!(create_execution_client, m)?)?;

    Ok(())
}

// ── Credential ────────────────────────────────────────────────────────────────

/// Schwab OAuth credentials for API authentication.
///
/// Provides multiple ways to obtain credentials:
/// - Explicit construction with all five fields
/// - `from_env()` to read from environment variables
/// - `from_schwab_mcp()` to reuse existing schwab-mcp OAuth setup
///
/// Environment variables (for `from_env()`):
///     SCHWAB_APP_KEY, SCHWAB_APP_SECRET, SCHWAB_CALLBACK_URL,
///     SCHWAB_ACCESS_TOKEN, SCHWAB_REFRESH_TOKEN
///
/// Example:
///     >>> cred = SchwabCredential.from_env()
///     >>> cred = SchwabCredential.from_schwab_mcp()
///     >>> cred = SchwabCredential("key", "secret", "https://...", "access", "refresh")
#[pyclass(name = "SchwabCredential", from_py_object)]
#[derive(Clone)]
pub struct PySchwabCredential {
    inner: crate::common::credential::SchwabCredential,
}

#[pymethods]
impl PySchwabCredential {
    /// Create a new SchwabCredential with explicit values.
    ///
    /// Args:
    ///     app_key: Schwab application key (client ID).
    ///     app_secret: Schwab application secret.
    ///     callback_url: OAuth callback URL (e.g. "https://127.0.0.1:8182").
    ///     access_token: Current OAuth access token.
    ///     refresh_token: OAuth refresh token for automatic renewal.
    #[new]
    fn new(
        app_key: String,
        app_secret: String,
        callback_url: String,
        access_token: String,
        refresh_token: String,
    ) -> Self {
        Self {
            inner: crate::common::credential::SchwabCredential::new(
                app_key,
                app_secret,
                callback_url,
                access_token,
                refresh_token,
            ),
        }
    }

    /// Load credentials from environment variables.
    ///
    /// Reads: SCHWAB_APP_KEY, SCHWAB_APP_SECRET, SCHWAB_CALLBACK_URL,
    /// SCHWAB_ACCESS_TOKEN, SCHWAB_REFRESH_TOKEN.
    ///
    /// Returns:
    ///     SchwabCredential populated from environment.
    ///
    /// Raises:
    ///     RuntimeError: If any required variable is missing.
    #[staticmethod]
    fn from_env() -> PyResult<Self> {
        let inner = crate::common::credential::SchwabCredential::from_env()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Load credentials from existing schwab-mcp infrastructure.
    ///
    /// Reads tokens from ``~/.local/share/schwab-mcp/token.yaml`` and
    /// credentials from ``~/.local/share/schwab-mcp/credentials.yaml``.
    /// This integrates with the OAuth setup used by finrl-trading.
    ///
    /// Returns:
    ///     SchwabCredential populated from schwab-mcp files.
    ///
    /// Raises:
    ///     FileNotFoundError: If token or credentials file is missing.
    ///     ValueError: If required fields are missing or YAML is malformed.
    #[staticmethod]
    fn from_schwab_mcp() -> PyResult<Self> {
        let token_path = shellexpand::tilde("~/.local/share/schwab-mcp/token.yaml").into_owned();
        let creds_path =
            shellexpand::tilde("~/.local/share/schwab-mcp/credentials.yaml").into_owned();

        // Read token.yaml
        let token_content = std::fs::read_to_string(&token_path).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyFileNotFoundError, _>(format!(
                "cannot read token file {}: {}",
                token_path, e
            ))
        })?;
        let token_doc: serde_yaml::Value =
            serde_yaml::from_str(&token_content).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "failed to parse token.yaml: {}",
                    e
                ))
            })?;

        let token = token_doc.get("token").unwrap_or(&token_doc);
        let access_token = token["access_token"]
            .as_str()
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "missing access_token in token.yaml",
                )
            })?;
        let refresh_token = token["refresh_token"]
            .as_str()
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "missing refresh_token in token.yaml",
                )
            })?;

        // Read credentials.yaml
        let creds_content = std::fs::read_to_string(&creds_path).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyFileNotFoundError, _>(format!(
                "cannot read credentials file {}: {}",
                creds_path, e
            ))
        })?;
        let creds_doc: serde_yaml::Value =
            serde_yaml::from_str(&creds_content).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "failed to parse credentials.yaml: {}",
                    e
                ))
            })?;

        let app_key = creds_doc["app_key"]
            .as_str()
            .or_else(|| creds_doc["client_id"].as_str())
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "missing app_key in credentials.yaml",
                )
            })?;
        let app_secret = creds_doc["app_secret"]
            .as_str()
            .or_else(|| creds_doc["client_secret"].as_str())
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "missing app_secret in credentials.yaml",
                )
            })?;
        let callback_url = creds_doc["callback_url"]
            .as_str()
            .unwrap_or("https://127.0.0.1:8182");

        let inner = crate::common::credential::SchwabCredential::new(
            app_key,
            app_secret,
            callback_url,
            access_token,
            refresh_token,
        );

        Ok(Self { inner })
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

// ── Data Client Config ────────────────────────────────────────────────────────

/// Configuration for the Schwab data client.
///
/// Controls how the data client identifies itself within the Nautilus engine.
///
/// Args:
///     client_id: Identifier string for this client (default: "SCHWAB").
///
/// Example:
///     >>> config = SchwabDataClientConfig()
///     >>> config = SchwabDataClientConfig(client_id="MY-SCHWAB")
///     >>> print(config.client_id)
///     'MY-SCHWAB'
#[pyclass(name = "SchwabDataClientConfig", from_py_object)]
#[derive(Clone)]
pub struct PySchwabDataClientConfig {
    inner: crate::data::SchwabDataClientConfig,
}

#[pymethods]
impl PySchwabDataClientConfig {
    /// Create a new SchwabDataClientConfig.
    ///
    /// Args:
    ///     client_id: Client identifier (default: "SCHWAB").
    #[new]
    #[pyo3(signature = (client_id="SCHWAB".to_string()))]
    fn new(client_id: String) -> Self {
        Self {
            inner: crate::data::SchwabDataClientConfig { client_id },
        }
    }

    /// The client identifier string.
    #[getter]
    fn client_id(&self) -> &str {
        &self.inner.client_id
    }

    fn __repr__(&self) -> String {
        format!("SchwabDataClientConfig(client_id='{}')", self.inner.client_id)
    }
}

// ── Execution Client Config ───────────────────────────────────────────────────

/// Configuration for the Schwab execution client.
///
/// Controls client identity, account selection, and default account number.
///
/// Args:
///     client_id: Identifier string for this client (default: "SCHWAB").
///     account_id: Nautilus account identifier (default: "SCHWAB-001").
///     default_account: Specific Schwab account number to use, or None
///         to auto-select the first available account.
///
/// Example:
///     >>> config = SchwabExecutionClientConfig()
///     >>> config = SchwabExecutionClientConfig(
///     ...     client_id="SCHWAB",
///     ...     account_id="SCHWAB-001",
///     ...     default_account="12345678",
///     ... )
#[pyclass(name = "SchwabExecutionClientConfig", from_py_object)]
#[derive(Clone)]
pub struct PySchwabExecutionClientConfig {
    inner: crate::execution::SchwabExecutionClientConfig,
}

#[pymethods]
impl PySchwabExecutionClientConfig {
    /// Create a new SchwabExecutionClientConfig.
    ///
    /// Args:
    ///     client_id: Client identifier (default: "SCHWAB").
    ///     account_id: Account identifier (default: "SCHWAB-001").
    ///     default_account: Optional specific account number.
    #[new]
    #[pyo3(signature = (client_id="SCHWAB".to_string(), account_id="SCHWAB-001".to_string(), default_account=None))]
    fn new(client_id: String, account_id: String, default_account: Option<String>) -> Self {
        Self {
            inner: crate::execution::SchwabExecutionClientConfig {
                client_id,
                account_id,
                default_account,
            },
        }
    }

    /// The client identifier string.
    #[getter]
    fn client_id(&self) -> &str {
        &self.inner.client_id
    }

    /// The Nautilus account identifier.
    #[getter]
    fn account_id(&self) -> &str {
        &self.inner.account_id
    }

    /// The default Schwab account number, if configured.
    #[getter]
    fn default_account(&self) -> Option<&str> {
        self.inner.default_account.as_deref()
    }

    fn __repr__(&self) -> String {
        format!(
            "SchwabExecutionClientConfig(client_id='{}', account_id='{}', default_account={:?})",
            self.inner.client_id, self.inner.account_id, self.inner.default_account
        )
    }
}

// ── Data Client Factory ───────────────────────────────────────────────────────

/// Factory for creating Schwab data client instances.
///
/// Implements the Nautilus ``DataClientFactory`` interface, allowing the live
/// system kernel to instantiate ``SchwabDataClient`` from configuration.
///
/// This is the primary integration point for the Nautilus plugin system.
/// Register this factory with the engine to enable automatic client creation.
///
/// Example:
///     >>> factory = SchwabDataClientFactory()
///     >>> print(factory.name)
///     'SCHWAB'
///     >>> print(factory.config_type)
///     'SchwabDataClientConfig'
#[pyclass(name = "SchwabDataClientFactory", from_py_object)]
#[derive(Clone)]
pub struct PySchwabDataClientFactory;

#[pymethods]
impl PySchwabDataClientFactory {
    /// Create a new SchwabDataClientFactory instance.
    #[new]
    fn new() -> Self {
        Self
    }

    /// Factory name used for registration with the Nautilus engine.
    #[getter]
    fn name(&self) -> &str {
        "SCHWAB"
    }

    /// Expected configuration type name for this factory.
    #[getter]
    fn config_type(&self) -> &str {
        "SchwabDataClientConfig"
    }

    fn __repr__(&self) -> String {
        "SchwabDataClientFactory(name='SCHWAB')".to_string()
    }
}

// ── Execution Client Factory ──────────────────────────────────────────────────

/// Factory for creating Schwab execution client instances.
///
/// Implements the Nautilus ``ExecutionClientFactory`` interface, allowing the
/// live system kernel to instantiate ``SchwabExecutionClient`` from configuration.
///
/// This is the primary integration point for the Nautilus plugin system.
/// Register this factory with the engine to enable automatic client creation.
///
/// Example:
///     >>> factory = SchwabExecutionClientFactory()
///     >>> print(factory.name)
///     'SCHWAB'
///     >>> print(factory.config_type)
///     'SchwabExecutionClientConfig'
#[pyclass(name = "SchwabExecutionClientFactory", from_py_object)]
#[derive(Clone)]
pub struct PySchwabExecutionClientFactory;

#[pymethods]
impl PySchwabExecutionClientFactory {
    /// Create a new SchwabExecutionClientFactory instance.
    #[new]
    fn new() -> Self {
        Self
    }

    /// Factory name used for registration with the Nautilus engine.
    #[getter]
    fn name(&self) -> &str {
        "SCHWAB"
    }

    /// Expected configuration type name for this factory.
    #[getter]
    fn config_type(&self) -> &str {
        "SchwabExecutionClientConfig"
    }

    fn __repr__(&self) -> String {
        "SchwabExecutionClientFactory(name='SCHWAB')".to_string()
    }
}

// ── Legacy convenience functions ──────────────────────────────────────────────

/// Create a Schwab data client (legacy convenience function).
///
/// Validates that a data client can be constructed from the given credential
/// and configuration. Returns a confirmation string on success.
///
/// .. note::
///     Prefer using ``SchwabDataClientFactory`` for production integration
///     with the Nautilus engine.
///
/// Args:
///     credential: SchwabCredential for API authentication.
///     config: SchwabDataClientConfig for client settings.
///
/// Returns:
///     str: Confirmation message with client details.
///
/// Raises:
///     RuntimeError: If client creation fails.
#[pyfunction]
fn create_data_client(
    credential: &PySchwabCredential,
    config: &PySchwabDataClientConfig,
) -> PyResult<String> {
    let _client = crate::factories::create_data_client(
        credential.inner.clone(),
        config.inner.clone(),
    )
    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    Ok(format!(
        "SchwabDataClient created (client_id={})",
        config.inner.client_id
    ))
}

/// Create a Schwab execution client (legacy convenience function).
///
/// Validates that an execution client can be constructed from the given
/// credential and configuration. Returns a confirmation string on success.
///
/// .. note::
///     Prefer using ``SchwabExecutionClientFactory`` for production integration
///     with the Nautilus engine.
///
/// Args:
///     credential: SchwabCredential for API authentication.
///     config: SchwabExecutionClientConfig for client settings.
///
/// Returns:
///     str: Confirmation message with client details.
///
/// Raises:
///     RuntimeError: If client creation fails.
#[pyfunction]
fn create_execution_client(
    credential: &PySchwabCredential,
    config: &PySchwabExecutionClientConfig,
) -> PyResult<String> {
    let _client = crate::factories::create_execution_client(
        credential.inner.clone(),
        config.inner.clone(),
    )
    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    Ok(format!(
        "SchwabExecutionClient created (client_id={}, account_id={})",
        config.inner.client_id, config.inner.account_id
    ))
}

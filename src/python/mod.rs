//! PyO3 Python bindings for the Schwab adapter.
//!
//! Exposes Rust types to Python for use within Nautilus Trader's
//! Python-based strategy and configuration layer.

use pyo3::prelude::*;

/// Python module initialization.
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

/// Python wrapper for SchwabCredential.
#[pyclass(name = "SchwabCredential", from_py_object)]
#[derive(Clone)]
pub struct PySchwabCredential {
    inner: crate::common::credential::SchwabCredential,
}

#[pymethods]
impl PySchwabCredential {
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
    /// Expected variables: SCHWAB_APP_KEY, SCHWAB_APP_SECRET,
    /// SCHWAB_CALLBACK_URL, SCHWAB_ACCESS_TOKEN, SCHWAB_REFRESH_TOKEN
    #[staticmethod]
    fn from_env() -> PyResult<Self> {
        let inner = crate::common::credential::SchwabCredential::from_env()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Load credentials from existing schwab-mcp infrastructure.
    ///
    /// Reads tokens from ~/.local/share/schwab-mcp/token.yaml and
    /// credentials from ~/.local/share/schwab-mcp/credentials.yaml.
    /// This integrates with the OAuth setup used by finrl-trading.
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

/// Python wrapper for SchwabDataClientConfig.
#[pyclass(name = "SchwabDataClientConfig", from_py_object)]
#[derive(Clone)]
pub struct PySchwabDataClientConfig {
    inner: crate::data::SchwabDataClientConfig,
}

#[pymethods]
impl PySchwabDataClientConfig {
    #[new]
    #[pyo3(signature = (client_id="SCHWAB".to_string()))]
    fn new(client_id: String) -> Self {
        Self {
            inner: crate::data::SchwabDataClientConfig { client_id },
        }
    }

    /// Get the client ID.
    #[getter]
    fn client_id(&self) -> &str {
        &self.inner.client_id
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

// ── Execution Client Config ───────────────────────────────────────────────────

/// Python wrapper for SchwabExecutionClientConfig.
#[pyclass(name = "SchwabExecutionClientConfig", from_py_object)]
#[derive(Clone)]
pub struct PySchwabExecutionClientConfig {
    inner: crate::execution::SchwabExecutionClientConfig,
}

#[pymethods]
impl PySchwabExecutionClientConfig {
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

    /// Get the client ID.
    #[getter]
    fn client_id(&self) -> &str {
        &self.inner.client_id
    }

    /// Get the account ID.
    #[getter]
    fn account_id(&self) -> &str {
        &self.inner.account_id
    }

    /// Get the default account number.
    #[getter]
    fn default_account(&self) -> Option<&str> {
        self.inner.default_account.as_deref()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

// ── Data Client Factory ───────────────────────────────────────────────────────

/// Python wrapper for SchwabDataClientFactory.
///
/// Implements the Nautilus DataClientFactory interface, allowing the live
/// system kernel to instantiate SchwabDataClient from configuration.
#[pyclass(name = "SchwabDataClientFactory", from_py_object)]
#[derive(Clone)]
pub struct PySchwabDataClientFactory;

#[pymethods]
impl PySchwabDataClientFactory {
    #[new]
    fn new() -> Self {
        Self
    }

    /// Factory name used for registration.
    #[getter]
    fn name(&self) -> &str {
        "SCHWAB"
    }

    /// Expected config type name.
    #[getter]
    fn config_type(&self) -> &str {
        "SchwabDataClientConfig"
    }

    fn __repr__(&self) -> String {
        "SchwabDataClientFactory(name='SCHWAB')".to_string()
    }
}

// ── Execution Client Factory ──────────────────────────────────────────────────

/// Python wrapper for SchwabExecutionClientFactory.
///
/// Implements the Nautilus ExecutionClientFactory interface, allowing the live
/// system kernel to instantiate SchwabExecutionClient from configuration.
#[pyclass(name = "SchwabExecutionClientFactory", from_py_object)]
#[derive(Clone)]
pub struct PySchwabExecutionClientFactory;

#[pymethods]
impl PySchwabExecutionClientFactory {
    #[new]
    fn new() -> Self {
        Self
    }

    /// Factory name used for registration.
    #[getter]
    fn name(&self) -> &str {
        "SCHWAB"
    }

    /// Expected config type name.
    #[getter]
    fn config_type(&self) -> &str {
        "SchwabExecutionClientConfig"
    }

    fn __repr__(&self) -> String {
        "SchwabExecutionClientFactory(name='SCHWAB')".to_string()
    }
}

// ── Legacy convenience functions ──────────────────────────────────────────────

/// Create a Schwab data client from Python.
///
/// Legacy convenience function. Prefer using SchwabDataClientFactory
/// for production integration with the Nautilus engine.
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

/// Create a Schwab execution client from Python.
///
/// Legacy convenience function. Prefer using SchwabExecutionClientFactory
/// for production integration with the Nautilus engine.
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

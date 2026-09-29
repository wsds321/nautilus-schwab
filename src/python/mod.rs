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

    // Register factory functions
    m.add_function(wrap_pyfunction!(create_data_client, m)?)?;
    m.add_function(wrap_pyfunction!(create_execution_client, m)?)?;

    Ok(())
}

/// Python wrapper for SchwabCredential.
#[pyclass(name = "SchwabCredential")]
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
    #[staticmethod]
    fn from_env() -> PyResult<Self> {
        let inner = crate::common::credential::SchwabCredential::from_env()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(Self { inner })
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

/// Python wrapper for SchwabDataClientConfig.
#[pyclass(name = "SchwabDataClientConfig")]
#[derive(Clone)]
pub struct PySchwabDataClientConfig {
    inner: crate::data::SchwabDataClientConfig,
}

#[pymethods]
impl PySchwabDataClientConfig {
    #[new]
    fn new() -> Self {
        Self {
            inner: crate::data::SchwabDataClientConfig::default(),
        }
    }
}

/// Python wrapper for SchwabExecutionClientConfig.
#[pyclass(name = "SchwabExecutionClientConfig")]
#[derive(Clone)]
pub struct PySchwabExecutionClientConfig {
    inner: crate::execution::SchwabExecutionClientConfig,
}

#[pymethods]
impl PySchwabExecutionClientConfig {
    #[new]
    fn new() -> Self {
        Self {
            inner: crate::execution::SchwabExecutionClientConfig::default(),
        }
    }
}

/// Create a Schwab data client from Python.
#[pyfunction]
fn create_data_client(
    credential: &PySchwabCredential,
    config: &PySchwabDataClientConfig,
) -> PyResult<String> {
    // TODO: Return actual client handle
    Ok("SchwabDataClient created".to_string())
}

/// Create a Schwab execution client from Python.
#[pyfunction]
fn create_execution_client(
    credential: &PySchwabCredential,
    config: &PySchwabExecutionClientConfig,
) -> PyResult<String> {
    // TODO: Return actual client handle
    Ok("SchwabExecutionClient created".to_string())
}

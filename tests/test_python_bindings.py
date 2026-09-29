"""Tests for Python bindings of the nautilus-schwab adapter.

These tests verify that the PyO3 bindings are correctly exposed and that
basic instantiation works. They do NOT require valid Schwab credentials
or network access — they only test the Python interface surface.

Run with:
    pytest tests/test_python_bindings.py -v

Skip if maturin build is not available:
    pytest tests/test_python_bindings.py -v --skip-no-build
"""

import pytest


@pytest.fixture(scope="module")
def ns():
    """Import the nautilus_schwab module, skipping if not built."""
    try:
        import nautilus_schwab
        return nautilus_schwab
    except ImportError as e:
        pytest.skip(f"nautilus_schwab not installed (run `maturin develop`): {e}")


# ── Module-level exports ──────────────────────────────────────────────────────


class TestModuleExports:
    """Verify all expected symbols are exported from the package."""

    def test_version_string(self, ns):
        assert hasattr(ns, "__version__")
        assert isinstance(ns.__version__, str)
        assert ns.__version__ == "0.1.0"

    def test_all_list(self, ns):
        assert hasattr(ns, "__all__")
        expected = {
            "SchwabCredential",
            "SchwabDataClientConfig",
            "SchwabExecutionClientConfig",
            "SchwabDataClientFactory",
            "SchwabExecutionClientFactory",
            "create_data_client",
            "create_execution_client",
        }
        assert set(ns.__all__) == expected

    def test_all_symbols_importable(self, ns):
        for name in ns.__all__:
            assert hasattr(ns, name), f"{name} missing from module"


# ── SchwabCredential ──────────────────────────────────────────────────────────


class TestSchwabCredential:
    """Test SchwabCredential construction and methods."""

    def test_explicit_construction(self, ns):
        cred = ns.SchwabCredential(
            app_key="test-key",
            app_secret="test-secret",
            callback_url="https://127.0.0.1:8182",
            access_token="test-access",
            refresh_token="test-refresh",
        )
        assert cred is not None

    def test_repr(self, ns):
        cred = ns.SchwabCredential(
            "key", "secret", "https://localhost", "access", "refresh"
        )
        r = repr(cred)
        assert isinstance(r, str)
        assert len(r) > 0

    def test_from_env_missing_raises(self, ns):
        """from_env() should raise when env vars are not set."""
        import os

        # Save and clear any existing Schwab env vars
        saved = {}
        for var in [
            "SCHWAB_APP_KEY",
            "SCHWAB_APP_SECRET",
            "SCHWAB_CALLBACK_URL",
            "SCHWAB_ACCESS_TOKEN",
            "SCHWAB_REFRESH_TOKEN",
        ]:
            saved[var] = os.environ.pop(var, None)

        try:
            with pytest.raises(RuntimeError):
                ns.SchwabCredential.from_env()
        finally:
            # Restore original values
            for var, val in saved.items():
                if val is not None:
                    os.environ[var] = val

    def test_from_schwab_mcp_missing_raises(self, ns):
        """from_schwab_mcp() should raise when files don't exist."""
        with pytest.raises(FileNotFoundError):
            ns.SchwabCredential.from_schwab_mcp()


# ── SchwabDataClientConfig ────────────────────────────────────────────────────


class TestSchwabDataClientConfig:
    """Test SchwabDataClientConfig construction and properties."""

    def test_default_construction(self, ns):
        config = ns.SchwabDataClientConfig()
        assert config.client_id == "SCHWAB"

    def test_custom_client_id(self, ns):
        config = ns.SchwabDataClientConfig(client_id="MY-SCHWAB")
        assert config.client_id == "MY-SCHWAB"

    def test_repr(self, ns):
        config = ns.SchwabDataClientConfig()
        r = repr(config)
        assert "SCHWAB" in r
        assert "SchwabDataClientConfig" in r


# ── SchwabExecutionClientConfig ───────────────────────────────────────────────


class TestSchwabExecutionClientConfig:
    """Test SchwabExecutionClientConfig construction and properties."""

    def test_default_construction(self, ns):
        config = ns.SchwabExecutionClientConfig()
        assert config.client_id == "SCHWAB"
        assert config.account_id == "SCHWAB-001"
        assert config.default_account is None

    def test_custom_values(self, ns):
        config = ns.SchwabExecutionClientConfig(
            client_id="MY-SCHWAB",
            account_id="SCHWAB-002",
            default_account="12345678",
        )
        assert config.client_id == "MY-SCHWAB"
        assert config.account_id == "SCHWAB-002"
        assert config.default_account == "12345678"

    def test_partial_custom(self, ns):
        config = ns.SchwabExecutionClientConfig(account_id="ACCT-99")
        assert config.client_id == "SCHWAB"  # default
        assert config.account_id == "ACCT-99"
        assert config.default_account is None

    def test_repr(self, ns):
        config = ns.SchwabExecutionClientConfig()
        r = repr(config)
        assert "SCHWAB" in r
        assert "SCHWAB-001" in r
        assert "SchwabExecutionClientConfig" in r


# ── Factory classes ───────────────────────────────────────────────────────────


class TestFactories:
    """Test factory class construction and metadata."""

    def test_data_factory_creation(self, ns):
        factory = ns.SchwabDataClientFactory()
        assert factory is not None

    def test_data_factory_name(self, ns):
        factory = ns.SchwabDataClientFactory()
        assert factory.name == "SCHWAB"

    def test_data_factory_config_type(self, ns):
        factory = ns.SchwabDataClientFactory()
        assert factory.config_type == "SchwabDataClientConfig"

    def test_data_factory_repr(self, ns):
        factory = ns.SchwabDataClientFactory()
        assert "SCHWAB" in repr(factory)

    def test_execution_factory_creation(self, ns):
        factory = ns.SchwabExecutionClientFactory()
        assert factory is not None

    def test_execution_factory_name(self, ns):
        factory = ns.SchwabExecutionClientFactory()
        assert factory.name == "SCHWAB"

    def test_execution_factory_config_type(self, ns):
        factory = ns.SchwabExecutionClientFactory()
        assert factory.config_type == "SchwabExecutionClientConfig"

    def test_execution_factory_repr(self, ns):
        factory = ns.SchwabExecutionClientFactory()
        assert "SCHWAB" in repr(factory)


# ── Legacy convenience functions ──────────────────────────────────────────────


class TestLegacyFunctions:
    """Test legacy create_* convenience functions."""

    def test_create_data_client_callable(self, ns):
        assert callable(ns.create_data_client)

    def test_create_execution_client_callable(self, ns):
        assert callable(ns.create_execution_client)

    def test_create_data_client_with_valid_args(self, ns):
        """create_data_client should succeed with valid credential + config."""
        cred = ns.SchwabCredential(
            "key", "secret", "https://localhost", "access", "refresh"
        )
        config = ns.SchwabDataClientConfig()
        result = ns.create_data_client(cred, config)
        assert isinstance(result, str)
        assert "SCHWAB" in result

    def test_create_execution_client_with_valid_args(self, ns):
        """create_execution_client should succeed with valid credential + config."""
        cred = ns.SchwabCredential(
            "key", "secret", "https://localhost", "access", "refresh"
        )
        config = ns.SchwabExecutionClientConfig()
        result = ns.create_execution_client(cred, config)
        assert isinstance(result, str)
        assert "SCHWAB" in result

//! Bidirectional symbol mapping between Schwab and Nautilus InstrumentId.
//!
//! Supports multiple instrument types with their native formats:
//! - Equity: `AAPL` → `AAPL.SCHWAB`
//! - Option (OCC): `AAPL240119C00150000` → `AAPL240119C00150000.SCHWAB`
//! - Future: `/ES` → `/ES.SCHWAB`
//! - Forex: `EUR/USD` → `EUR/USD.SCHWAB`

use std::fmt;

use super::enums::SchwabAssetType;

/// Wrapper for Schwab-native symbols with validation and asset type tracking.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchwabSymbol {
    /// The raw Schwab ticker string (uppercase-normalized).
    symbol: String,
    /// The asset type this symbol represents.
    asset_type: SchwabAssetType,
}

impl SchwabSymbol {
    /// Create a new SchwabSymbol from a ticker string, auto-detecting asset type.
    ///
    /// Detection rules (applied in order):
    /// - Starts with `/` → Future
    /// - Contains `/` but doesn't start with it → Forex
    /// - Matches OCC option format (6+ chars after root: YYMMDD + C/P + 8-digit strike) → Option
    /// - Otherwise → Equity
    ///
    /// Validates that the symbol is non-empty and contains only valid characters.
    pub fn new(symbol: impl Into<String>) -> Result<Self, SymbolError> {
        let s = symbol.into();
        if s.is_empty() {
            return Err(SymbolError::Empty);
        }
        let upper = s.to_uppercase();

        // Validate allowed characters: alphanumeric, dots, hyphens, slashes
        if !upper
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '/')
        {
            return Err(SymbolError::InvalidChars(upper));
        }

        let asset_type = detect_asset_type(&upper);
        validate_symbol_format(&upper, asset_type)?;

        Ok(Self {
            symbol: upper,
            asset_type,
        })
    }

    /// Create a new SchwabSymbol with an explicit asset type.
    ///
    /// Use this when the asset type is known from context (e.g., API response)
    /// and should not be auto-detected.
    pub fn with_asset_type(
        symbol: impl Into<String>,
        asset_type: SchwabAssetType,
    ) -> Result<Self, SymbolError> {
        let s = symbol.into();
        if s.is_empty() {
            return Err(SymbolError::Empty);
        }
        let upper = s.to_uppercase();

        if !upper
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '/')
        {
            return Err(SymbolError::InvalidChars(upper));
        }

        validate_symbol_format(&upper, asset_type)?;

        Ok(Self {
            symbol: upper,
            asset_type,
        })
    }

    /// Get the raw Schwab ticker string.
    pub fn as_str(&self) -> &str {
        &self.symbol
    }

    /// Get the asset type of this symbol.
    pub fn asset_type(&self) -> SchwabAssetType {
        self.asset_type
    }

    /// Convert to Nautilus InstrumentId string format: `{SYMBOL}.SCHWAB`
    pub fn to_instrument_id(&self) -> String {
        format!("{}.SCHWAB", self.symbol)
    }

    /// Parse a Nautilus InstrumentId string back to SchwabSymbol.
    ///
    /// Expects format `{SYMBOL}.SCHWAB`. Asset type is auto-detected from
    /// the symbol format.
    pub fn from_instrument_id(instrument_id: &str) -> Result<Self, SymbolError> {
        let Some(symbol) = instrument_id.strip_suffix(".SCHWAB") else {
            return Err(SymbolError::WrongVenue(instrument_id.to_string()));
        };
        Self::new(symbol)
    }
}

/// Detect asset type from symbol format.
fn detect_asset_type(symbol: &str) -> SchwabAssetType {
    if symbol.starts_with('/') {
        SchwabAssetType::Future
    } else if symbol.contains('/') && !symbol.starts_with('/') {
        SchwabAssetType::Forex
    } else if is_occ_option_format(symbol) {
        SchwabAssetType::Option
    } else {
        SchwabAssetType::Equity
    }
}

/// Check if a symbol matches the OCC (Options Clearing Corporation) format.
///
/// OCC format: `{ROOT}{YYMMDD}{C|P}{STRIKE*1000}`
/// Example: `AAPL240119C00150000` (AAPL Jan 19 2024 Call $150.00)
///
/// The root is variable length (1-6 chars), followed by exactly 6 date digits,
/// one C/P character, and exactly 8 strike digits.
fn is_occ_option_format(symbol: &str) -> bool {
    let len = symbol.len();
    // Minimum: 1-char root + 6 date + 1 CP + 8 strike = 16
    if len < 16 {
        return false;
    }

    // Find where the numeric suffix starts: last 15 chars must be 6 digits + CP + 8 digits
    let suffix_start = len - 15;
    let suffix = &symbol[suffix_start..];

    // First 6 chars of suffix: date digits (YYMMDD)
    let date_part = &suffix[..6];
    if !date_part.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    // 7th char: C or P
    let cp = suffix.as_bytes()[6];
    if cp != b'C' && cp != b'P' {
        return false;
    }

    // Last 8 chars: strike price * 1000 (zero-padded)
    let strike_part = &suffix[7..];
    if !strike_part.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    // Root part must be alphabetic (allow dots/hyphens for BRK.B style roots)
    let root = &symbol[..suffix_start];
    !root.is_empty()
        && root
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '.' || c == '-')
}

/// Validate symbol format for a given asset type.
fn validate_symbol_format(symbol: &str, asset_type: SchwabAssetType) -> Result<(), SymbolError> {
    match asset_type {
        SchwabAssetType::Future => {
            if !symbol.starts_with('/') {
                return Err(SymbolError::InvalidFormat(
                    symbol.to_string(),
                    "future symbols must start with '/'".to_string(),
                ));
            }
            // Must have at least one char after the slash
            if symbol.len() < 2 {
                return Err(SymbolError::InvalidFormat(
                    symbol.to_string(),
                    "future symbol must have content after '/'".to_string(),
                ));
            }
        }
        SchwabAssetType::Option => {
            if !is_occ_option_format(symbol) {
                return Err(SymbolError::InvalidFormat(
                    symbol.to_string(),
                    "option symbol must be in OCC format (e.g., AAPL240119C00150000)".to_string(),
                ));
            }
        }
        SchwabAssetType::Forex => {
            if !symbol.contains('/') {
                return Err(SymbolError::InvalidFormat(
                    symbol.to_string(),
                    "forex symbol must contain '/' separator (e.g., EUR/USD)".to_string(),
                ));
            }
            let parts: Vec<&str> = symbol.split('/').collect();
            if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
                return Err(SymbolError::InvalidFormat(
                    symbol.to_string(),
                    "forex symbol must be two currency codes separated by '/'".to_string(),
                ));
            }
        }
        SchwabAssetType::Equity | SchwabAssetType::Bond | SchwabAssetType::MutualFund => {
            // Standard equity/bond/mutual fund validation — no special format required
        }
        SchwabAssetType::Unknown => {
            // Allow any valid-character string for unknown types
        }
    }
    Ok(())
}

impl fmt::Display for SchwabSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol)
    }
}

impl AsRef<str> for SchwabSymbol {
    fn as_ref(&self) -> &str {
        &self.symbol
    }
}

/// Errors during symbol parsing or conversion.
#[derive(Debug, thiserror::Error)]
pub enum SymbolError {
    #[error("symbol cannot be empty")]
    Empty,

    #[error("symbol contains invalid characters: {0}")]
    InvalidChars(String),

    #[error("instrument ID does not belong to SCHWAB venue: {0}")]
    WrongVenue(String),

    #[error("unsupported product type for symbol: {0}")]
    UnsupportedProduct(String),

    #[error("invalid format for symbol '{0}': {1}")]
    InvalidFormat(String, String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Equity tests ───────────────────────────────────────────────────

    #[test]
    fn round_trip_equity() {
        let sym = SchwabSymbol::new("AAPL").unwrap();
        assert_eq!(sym.as_str(), "AAPL");
        assert_eq!(sym.asset_type(), SchwabAssetType::Equity);
        assert_eq!(sym.to_instrument_id(), "AAPL.SCHWAB");

        let parsed = SchwabSymbol::from_instrument_id("AAPL.SCHWAB").unwrap();
        assert_eq!(parsed, sym);
    }

    #[test]
    fn normalizes_to_uppercase() {
        let sym = SchwabSymbol::new("aapl").unwrap();
        assert_eq!(sym.as_str(), "AAPL");
    }

    #[test]
    fn rejects_empty() {
        assert!(SchwabSymbol::new("").is_err());
    }

    #[test]
    fn rejects_wrong_venue() {
        assert!(SchwabSymbol::from_instrument_id("AAPL.BINANCE").is_err());
    }

    #[test]
    fn handles_complex_tickers() {
        let sym = SchwabSymbol::new("BRK.B").unwrap();
        assert_eq!(sym.to_instrument_id(), "BRK.B.SCHWAB");
        assert_eq!(sym.asset_type(), SchwabAssetType::Equity);
    }

    #[test]
    fn rejects_special_characters() {
        assert!(SchwabSymbol::new("AAPL@").is_err());
        assert!(SchwabSymbol::new("A$APL").is_err());
        assert!(SchwabSymbol::new("AAP L").is_err());
        assert!(SchwabSymbol::new("AAPL!").is_err());
    }

    #[test]
    fn accepts_valid_special_chars() {
        assert!(SchwabSymbol::new("BRK.B").is_ok());
        assert!(SchwabSymbol::new("BF-B").is_ok());
    }

    #[test]
    fn display_impl() {
        let sym = SchwabSymbol::new("AAPL").unwrap();
        assert_eq!(format!("{}", sym), "AAPL");
    }

    #[test]
    fn as_ref_str() {
        let sym = SchwabSymbol::new("MSFT").unwrap();
        let s: &str = sym.as_ref();
        assert_eq!(s, "MSFT");
    }

    #[test]
    fn from_instrument_id_round_trip_with_dots() {
        let original = SchwabSymbol::new("BRK.B").unwrap();
        let id_str = original.to_instrument_id();
        assert_eq!(id_str, "BRK.B.SCHWAB");
        let parsed = SchwabSymbol::from_instrument_id(&id_str).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn from_instrument_id_rejects_no_suffix() {
        assert!(SchwabSymbol::from_instrument_id("AAPL").is_err());
    }

    #[test]
    fn equality_and_hash() {
        use std::collections::HashSet;
        let a = SchwabSymbol::new("AAPL").unwrap();
        let b = SchwabSymbol::new("aapl").unwrap();
        assert_eq!(a, b);

        let mut set = HashSet::new();
        set.insert(a.clone());
        assert!(set.contains(&b));
    }

    #[test]
    fn symbol_error_display_messages() {
        let e1 = SymbolError::Empty;
        assert!(format!("{}", e1).contains("empty"));

        let e2 = SymbolError::InvalidChars("BAD!".into());
        assert!(format!("{}", e2).contains("BAD!"));

        let e3 = SymbolError::WrongVenue("AAPL.BINANCE".into());
        assert!(format!("{}", e3).contains("BINANCE"));

        let e4 = SymbolError::UnsupportedProduct("CRYPTO".into());
        assert!(format!("{}", e4).contains("CRYPTO"));

        let e5 = SymbolError::InvalidFormat("BAD".into(), "reason".into());
        assert!(format!("{}", e5).contains("BAD"));
        assert!(format!("{}", e5).contains("reason"));
    }

    // ── Option (OCC format) tests ────────────────────────────────────────

    #[test]
    fn option_occ_format_detected() {
        let sym = SchwabSymbol::new("AAPL240119C00150000").unwrap();
        assert_eq!(sym.asset_type(), SchwabAssetType::Option);
        assert_eq!(sym.as_str(), "AAPL240119C00150000");
        assert_eq!(sym.to_instrument_id(), "AAPL240119C00150000.SCHWAB");
    }

    #[test]
    fn option_put_detected() {
        let sym = SchwabSymbol::new("SPY250321P00450000").unwrap();
        assert_eq!(sym.asset_type(), SchwabAssetType::Option);
    }

    #[test]
    fn option_round_trip() {
        let sym = SchwabSymbol::new("AAPL240119C00150000").unwrap();
        let id = sym.to_instrument_id();
        let parsed = SchwabSymbol::from_instrument_id(&id).unwrap();
        assert_eq!(parsed, sym);
        assert_eq!(parsed.asset_type(), SchwabAssetType::Option);
    }

    #[test]
    fn option_with_explicit_asset_type() {
        let sym =
            SchwabSymbol::with_asset_type("AAPL240119C00150000", SchwabAssetType::Option).unwrap();
        assert_eq!(sym.asset_type(), SchwabAssetType::Option);
    }

    #[test]
    fn option_invalid_occ_rejected() {
        // Too short for OCC format
        let result = SchwabSymbol::with_asset_type("AAPL2401", SchwabAssetType::Option);
        assert!(result.is_err());
    }

    #[test]
    fn is_occ_option_format_valid() {
        assert!(is_occ_option_format("AAPL240119C00150000"));
        assert!(is_occ_option_format("SPY250321P00450000"));
        assert!(is_occ_option_format("QQQ241220C00400000"));
        // Long root
        assert!(is_occ_option_format("BRK.B240119C00350000"));
    }

    #[test]
    fn is_occ_option_format_invalid() {
        // Too short
        assert!(!is_occ_option_format("AAPL240119C"));
        // No C/P
        assert!(!is_occ_option_format("AAPL240119X00150000"));
        // Non-digit strike
        assert!(!is_occ_option_format("AAPL240119C0015000A"));
        // Non-digit date
        assert!(!is_occ_option_format("AAPLABCDEFC00150000"));
        // Plain equity
        assert!(!is_occ_option_format("AAPL"));
    }

    // ── Future tests ─────────────────────────────────────────────────────

    #[test]
    fn future_format_detected() {
        let sym = SchwabSymbol::new("/ES").unwrap();
        assert_eq!(sym.asset_type(), SchwabAssetType::Future);
        assert_eq!(sym.as_str(), "/ES");
        assert_eq!(sym.to_instrument_id(), "/ES.SCHWAB");
    }

    #[test]
    fn future_round_trip() {
        let sym = SchwabSymbol::new("/NQ").unwrap();
        let id = sym.to_instrument_id();
        assert_eq!(id, "/NQ.SCHWAB");
        let parsed = SchwabSymbol::from_instrument_id(&id).unwrap();
        assert_eq!(parsed, sym);
        assert_eq!(parsed.asset_type(), SchwabAssetType::Future);
    }

    #[test]
    fn future_with_month_code() {
        let sym = SchwabSymbol::new("/ESH24").unwrap();
        assert_eq!(sym.asset_type(), SchwabAssetType::Future);
    }

    #[test]
    fn future_slash_only_rejected() {
        let result = SchwabSymbol::new("/");
        assert!(result.is_err());
    }

    #[test]
    fn future_explicit_asset_type_without_slash_rejected() {
        let result = SchwabSymbol::with_asset_type("ES", SchwabAssetType::Future);
        assert!(result.is_err());
    }

    // ── Forex tests ──────────────────────────────────────────────────────

    #[test]
    fn forex_format_detected() {
        let sym = SchwabSymbol::new("EUR/USD").unwrap();
        assert_eq!(sym.asset_type(), SchwabAssetType::Forex);
        assert_eq!(sym.as_str(), "EUR/USD");
        assert_eq!(sym.to_instrument_id(), "EUR/USD.SCHWAB");
    }

    #[test]
    fn forex_round_trip() {
        let sym = SchwabSymbol::new("GBP/JPY").unwrap();
        let id = sym.to_instrument_id();
        let parsed = SchwabSymbol::from_instrument_id(&id).unwrap();
        assert_eq!(parsed, sym);
        assert_eq!(parsed.asset_type(), SchwabAssetType::Forex);
    }

    #[test]
    fn forex_normalizes_case() {
        let sym = SchwabSymbol::new("eur/usd").unwrap();
        assert_eq!(sym.as_str(), "EUR/USD");
    }

    #[test]
    fn forex_explicit_without_slash_rejected() {
        let result = SchwabSymbol::with_asset_type("EURUSD", SchwabAssetType::Forex);
        assert!(result.is_err());
    }

    #[test]
    fn forex_empty_parts_rejected() {
        let result = SchwabSymbol::with_asset_type("/USD", SchwabAssetType::Forex);
        assert!(result.is_err());
        let result = SchwabSymbol::with_asset_type("EUR/", SchwabAssetType::Forex);
        assert!(result.is_err());
    }

    // ── with_asset_type tests ────────────────────────────────────────────

    #[test]
    fn with_asset_type_equity() {
        let sym = SchwabSymbol::with_asset_type("AAPL", SchwabAssetType::Equity).unwrap();
        assert_eq!(sym.asset_type(), SchwabAssetType::Equity);
        assert_eq!(sym.as_str(), "AAPL");
    }

    #[test]
    fn with_asset_type_rejects_empty() {
        let result = SchwabSymbol::with_asset_type("", SchwabAssetType::Equity);
        assert!(result.is_err());
    }

    #[test]
    fn with_asset_type_rejects_invalid_chars() {
        let result = SchwabSymbol::with_asset_type("AAPL@", SchwabAssetType::Equity);
        assert!(result.is_err());
    }
}

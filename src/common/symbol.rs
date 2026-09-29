//! Bidirectional symbol mapping between Schwab and Nautilus InstrumentId.
//!
//! Schwab uses plain ticker symbols (e.g., "AAPL") for equities.
//! Nautilus requires `InstrumentId` with venue suffix (e.g., "AAPL.SCHWAB").

use std::fmt;

/// Wrapper for Schwab-native symbols with validation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchwabSymbol(String);

impl SchwabSymbol {
    /// Create a new SchwabSymbol from a ticker string.
    ///
    /// Validates that the symbol is non-empty and contains only valid characters.
    pub fn new(symbol: impl Into<String>) -> Result<Self, SymbolError> {
        let s = symbol.into();
        if s.is_empty() {
            return Err(SymbolError::Empty);
        }
        // Schwab equity symbols: uppercase alphanumeric, dots, hyphens
        if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '/') {
            return Err(SymbolError::InvalidChars(s));
        }
        Ok(Self(s.to_uppercase()))
    }

    /// Get the raw Schwab ticker string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to Nautilus InstrumentId string format: `{SYMBOL}.SCHWAB`
    pub fn to_instrument_id(&self) -> String {
        format!("{}.SCHWAB", self.0)
    }

    /// Parse a Nautilus InstrumentId string back to SchwabSymbol.
    ///
    /// Expects format `{SYMBOL}.SCHWAB`.
    pub fn from_instrument_id(instrument_id: &str) -> Result<Self, SymbolError> {
        let Some(symbol) = instrument_id.strip_suffix(".SCHWAB") else {
            return Err(SymbolError::WrongVenue(instrument_id.to_string()));
        };
        Self::new(symbol)
    }
}

impl fmt::Display for SchwabSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for SchwabSymbol {
    fn as_ref(&self) -> &str {
        &self.0
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_equity() {
        let sym = SchwabSymbol::new("AAPL").unwrap();
        assert_eq!(sym.as_str(), "AAPL");
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
        // BRK.B style tickers
        let sym = SchwabSymbol::new("BRK.B").unwrap();
        assert_eq!(sym.to_instrument_id(), "BRK.B.SCHWAB");
    }

    // ── Additional validation tests ─────────────────────────────────────

    #[test]
    fn rejects_special_characters() {
        assert!(SchwabSymbol::new("AAPL@").is_err());
        assert!(SchwabSymbol::new("A$APL").is_err());
        assert!(SchwabSymbol::new("AAP L").is_err());
        assert!(SchwabSymbol::new("AAPL!").is_err());
    }

    #[test]
    fn accepts_valid_special_chars() {
        // Dots, hyphens, slashes are valid
        assert!(SchwabSymbol::new("BRK.B").is_ok());
        assert!(SchwabSymbol::new("BF-B").is_ok());
        assert!(SchwabSymbol::new("VFIAX/F").is_ok());
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
        let b = SchwabSymbol::new("aapl").unwrap(); // normalized to uppercase
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
    }
}

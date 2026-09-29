//! Integration tests for nautilus-schwab
//! 
//! These tests require valid Schwab credentials in ~/.local/share/schwab-mcp/
//! Run with: cargo test --test integration_test -- --ignored

use nautilus_schwab::common::symbol::SchwabSymbol;
use nautilus_schwab::http::client::SchwabHttpClient;
use nautilus_schwab::oauth::provider::SchwabTokenProvider;
use schwab_sdk::TokenProvider;
use std::sync::Arc;

/// Test symbol conversion (no network needed)
#[test]
fn test_symbol_conversion() {
    let sym = SchwabSymbol::new("AAPL").unwrap();
    let instrument_id = sym.to_instrument_id();
    assert_eq!(instrument_id.to_string(), "AAPL.SCHWAB");
    
    let back = SchwabSymbol::from_instrument_id(&instrument_id).unwrap();
    assert_eq!(back.as_ref(), "AAPL");
}

/// Test that we can load token provider from schwab-mcp
#[tokio::test]
#[ignore] // Requires real credentials
async fn test_load_token_provider_from_schwab_mcp() {
    let provider = SchwabTokenProvider::from_schwab_mcp()
        .expect("Failed to load token provider from schwab-mcp");
    
    // Verify we can get an access token (returns AuthToken, not String)
    let _token = provider.access_token()
        .expect("Failed to get access token");
    
    println!("✓ Loaded token provider successfully");
}

/// Test HTTP client creation
#[tokio::test]
#[ignore] // Requires real credentials
async fn test_http_client_creation() {
    let provider = Arc::new(
        SchwabTokenProvider::from_schwab_mcp()
            .expect("Failed to load token provider")
    );
    let _client = SchwabHttpClient::new(provider);
    
    println!("✓ HTTP client created successfully");
}

/// Test fetching account numbers via API
#[tokio::test]
#[ignore] // Requires real credentials and network
async fn test_fetch_accounts() {
    let provider = Arc::new(
        SchwabTokenProvider::from_schwab_mcp()
            .expect("Failed to load token provider")
    );
    let client = SchwabHttpClient::new(provider);
    
    // Call accounts API using inner() directly
    let result = client.inner().accounts().numbers().await;
    
    match result {
        Ok(accounts) => {
            println!("✓ Fetched {} accounts", accounts.len());
            for acc in &accounts {
                println!("  - Account: {:?}, Hash: {:?}", acc.account_number, acc.hash_value);
            }
            assert!(!accounts.is_empty(), "Should have at least one account");
        }
        Err(e) => {
            panic!("Failed to fetch accounts: {:?}", e);
        }
    }
}

/// Test fetching market quotes via API
#[tokio::test]
#[ignore] // Requires real credentials and network
async fn test_fetch_quotes() {
    let provider = Arc::new(
        SchwabTokenProvider::from_schwab_mcp()
            .expect("Failed to load token provider")
    );
    let client = SchwabHttpClient::new(provider);
    
    // Fetch quotes using inner() directly
    let result = client.inner()
        .market_data()
        .quotes()
        .list(["AAPL", "MSFT"])
        .send()
        .await;
    
    match result {
        Ok(quotes) => {
            println!("✓ Fetched quotes for {} symbols", quotes.len());
            for (symbol, entry) in &quotes {
                println!("  - {}: {:?}", symbol, entry);
            }
            assert!(quotes.contains_key("AAPL"), "Should have AAPL quote");
        }
        Err(e) => {
            panic!("Failed to fetch quotes: {:?}", e);
        }
    }
}

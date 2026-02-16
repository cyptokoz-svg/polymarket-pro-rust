//! Polymarket API clients
//! Gamma API for market data, CLOB API for trading

pub mod gamma;
pub mod clob;
pub mod market;

pub use gamma::GammaApiClient;
pub use clob::{ClobClient, ClobApiClient, Order, OrderResponse, OrderStatus};
pub use market::{MarketInfo, MarketToken, convert_market};

// Re-export Side from polymarket_client_sdk for consistency
pub use polymarket_client_sdk::clob::types::Side;

#[cfg(test)]
pub use clob::MockClobClient;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    #[error("Rate limited")]
    RateLimited,
}

/// Sanitize API error message to avoid leaking sensitive information
/// In production, returns generic error message
/// In debug mode, returns detailed error
pub fn sanitize_api_error(status: u16, detailed_message: String) -> ApiError {
    // In production builds, return generic error for 5xx server errors
    // to avoid leaking internal system details
    if cfg!(not(debug_assertions)) && status >= 500 {
        tracing::error!("API error {}: {}", status, detailed_message);
        return ApiError::ApiError {
            status,
            message: "Internal server error".to_string(),
        };
    }
    
    // For 4xx client errors, return the message (it's usually safe)
    // but still truncate very long messages
    let safe_message = if detailed_message.len() > 500 {
        format!("{}... (truncated)", &detailed_message[..500])
    } else {
        detailed_message
    };
    
    ApiError::ApiError {
        status,
        message: safe_message,
    }
}
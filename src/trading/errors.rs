//! Detailed error types for trading
//! Matches Python: RateLimitError, InsufficientBalanceError, etc.

use thiserror::Error;

/// Trading errors with detailed classification
#[derive(Error, Debug, Clone)]
pub enum TradingError {
    /// Rate limited by API
    #[error("Rate limited: {message}")]
    RateLimited { message: String },
    
    /// Insufficient balance
    #[error("Insufficient balance: available={available}, required={required}")]
    InsufficientBalance { available: f64, required: f64 },
    
    /// Market not found
    #[error("Market not found: {market_id}")]
    MarketNotFound { market_id: String },
    
    /// Order rejected
    #[error("Order rejected: {reason}")]
    OrderRejected { reason: String },
    
    /// Position limit exceeded
    #[error("Position limit exceeded: current={current}, new={new}, max={max}")]
    PositionLimitExceeded { current: f64, new: f64, max: f64 },
    
    /// Price out of range
    #[error("Price out of range: {price}")]
    PriceOutOfRange { price: f64 },
    
    /// Order not found
    #[error("Order not found: {order_id}")]
    OrderNotFound { order_id: String },
    
    /// API error
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    
    /// Network error
    #[error("Network error: {message}")]
    NetworkError { message: String },
    
    /// Timeout error
    #[error("Timeout error: {operation}")]
    TimeoutError { operation: String },
    
    /// Invalid order ID
    #[error("Invalid order ID: {order_id}")]
    InvalidOrderId { order_id: String },
    
    /// Wallet not authenticated
    #[error("Wallet not authenticated")]
    NotAuthenticated,
    
    /// Unknown error
    #[error("Unknown error: {message}")]
    Unknown { message: String },
}

impl TradingError {
    /// Check if error is retryable
    pub fn is_retryable(&self,
    ) -> bool {
        matches!(self,
            TradingError::RateLimited { .. } |
            TradingError::NetworkError { .. } |
            TradingError::TimeoutError { .. } |
            TradingError::ApiError { .. }
        )
    }
    
    /// Get error category for logging
    pub fn category(&self,
    ) -> &'static str {
        match self {
            TradingError::RateLimited { .. } => "RATE_LIMIT",
            TradingError::InsufficientBalance { .. } => "BALANCE",
            TradingError::MarketNotFound { .. } => "MARKET",
            TradingError::OrderRejected { .. } => "ORDER_REJECTED",
            TradingError::PositionLimitExceeded { .. } => "POSITION_LIMIT",
            TradingError::PriceOutOfRange { .. } => "PRICE",
            TradingError::OrderNotFound { .. } => "ORDER_NOT_FOUND",
            TradingError::ApiError { .. } => "API",
            TradingError::NetworkError { .. } => "NETWORK",
            TradingError::TimeoutError { .. } => "TIMEOUT",
            TradingError::InvalidOrderId { .. } => "INVALID_ORDER_ID",
            TradingError::NotAuthenticated => "AUTH",
            TradingError::Unknown { .. } => "UNKNOWN",
        }
    }
}

/// Convert generic error to TradingError
pub fn classify_error(
    err: Box<dyn std::error::Error>,
) -> TradingError {
    let err_msg = err.to_string();
    
    if err_msg.contains("rate limit") || err_msg.contains("Rate limit") {
        TradingError::RateLimited { message: err_msg }
    } else if err_msg.contains("balance") || err_msg.contains("Balance") {
        TradingError::InsufficientBalance { available: 0.0, required: 0.0 }
    } else if err_msg.contains("market") || err_msg.contains("Market") {
        TradingError::MarketNotFound { market_id: "".to_string() }
    } else if err_msg.contains("order") || err_msg.contains("Order") {
        TradingError::OrderRejected { reason: err_msg }
    } else if err_msg.contains("network") || err_msg.contains("Network") {
        TradingError::NetworkError { message: err_msg }
    } else if err_msg.contains("timeout") || err_msg.contains("Timeout") {
        TradingError::TimeoutError { operation: "".to_string() }
    } else {
        TradingError::Unknown { message: err_msg }
    }
}
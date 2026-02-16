//! Retry utilities for resilient API calls
//! 实现指数退避重试机制

use std::time::Duration;
use tracing::{warn, error};

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Initial delay in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 500,
            max_delay_ms: 10000,
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Create new retry config
    pub fn new(max_retries: u32, initial_delay_ms: u64) -> Self {
        Self {
            max_retries,
            initial_delay_ms,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
        }
    }
    
    /// No retry configuration
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            initial_delay_ms: 0,
            max_delay_ms: 0,
            backoff_multiplier: 1.0,
        }
    }
}

/// Retry a future with exponential backoff
pub async fn retry_with_backoff<F, Fut, T, E>(
    operation_name: &str,
    config: RetryConfig,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;
    let mut delay_ms = config.initial_delay_ms;
    
    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                
                if attempt < config.max_retries {
                    warn!(
                        "{} failed (attempt {}/{}), retrying in {}ms: {}",
                        operation_name,
                        attempt + 1,
                        config.max_retries + 1,
                        delay_ms,
                        last_error.as_ref().unwrap()
                    );
                    
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    
                    // Exponential backoff with max limit
                    delay_ms = ((delay_ms as f64 * config.backoff_multiplier) as u64)
                        .min(config.max_delay_ms);
                }
            }
        }
    }
    
    error!(
        "{} failed after {} attempts: {}",
        operation_name,
        config.max_retries + 1,
        last_error.as_ref().unwrap()
    );
    
    Err(last_error.unwrap())
}

/// Retry with default configuration
pub async fn retry<F, Fut, T, E>(
    operation_name: &str,
    operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    retry_with_backoff(operation_name, RetryConfig::default(), operation).await
}

/// Retry with custom max retries
pub async fn retry_n<F, Fut, T, E>(
    operation_name: &str,
    max_retries: u32,
    operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    retry_with_backoff(
        operation_name,
        RetryConfig::new(max_retries, 500),
        operation,
    ).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let result = retry("test", || async { Ok::<i32, &str>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let counter = AtomicU32::new(0);
        
        let result = retry_with_backoff(
            "test",
            RetryConfig::new(3, 10),
            || async {
                let count = counter.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err::<&str, &str>("fail")
                } else {
                    Ok("success")
                }
            },
        ).await;
        
        assert_eq!(result.unwrap(), "success");
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let counter = AtomicU32::new(0);
        
        let result = retry_with_backoff(
            "test",
            RetryConfig::new(2, 10),
            || async {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<&str, &str>("always fail")
            },
        ).await;
        
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3); // initial + 2 retries
    }
}
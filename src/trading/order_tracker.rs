//! Active order tracking
//! Matches Python: _active_orders, _track_order, _wait_for_fill

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{info, warn, debug};

/// Active order information
#[derive(Debug, Clone)]
pub struct ActiveOrder {
    pub order_id: String,
    pub token: String,
    pub side: String,
    pub price: f64,
    pub size: f64,
    pub timestamp: Instant,
}

/// Order tracker for managing active orders
pub struct OrderTracker {
    orders: HashMap<String, ActiveOrder>, // token -> order
}

impl OrderTracker {
    /// Create new order tracker
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
        }
    }

    /// Track a new order
    /// Matches Python: _track_order()
    pub fn track_order(
        &mut self,
        token: String,
        order_id: String,
        side: String,
        price: f64,
        size: f64,
    ) {
        let order = ActiveOrder {
            order_id: order_id.clone(),
            token: token.clone(),
            side,
            price,
            size,
            timestamp: Instant::now(),
        };

        info!("📋 Tracking order: {} for token {}", order_id, token);
        self.orders.insert(token, order);
    }

    /// Get active order for a token
    pub fn get_order(&self,
        token: &str,
    ) -> Option<&ActiveOrder> {
        self.orders.get(token)
    }

    /// Remove order from tracking
    pub fn remove_order(
        &mut self,
        token: &str,
    ) {
        if self.orders.remove(token).is_some() {
            debug!("Removed order tracking for token {}", token);
        }
    }

    /// Get all active orders
    pub fn get_all_orders(&self,
    ) -> &HashMap<String, ActiveOrder> {
        &self.orders
    }

    /// Find old orders (> threshold seconds)
    /// Matches Python: _cancel_old_pending_orders()
    pub fn find_old_orders(
        &self,
        threshold_secs: u64,
    ) -> Vec<&ActiveOrder> {
        let now = Instant::now();
        self.orders
            .values()
            .filter(|order| {
                let elapsed = now.duration_since(order.timestamp).as_secs();
                elapsed > threshold_secs
            })
            .collect()
    }

    /// Clear all tracked orders for a token
    /// Matches Python: _cancel_all_tracked_for_token()
    pub fn clear_orders_for_token(
        &mut self,
        token: &str,
    ) -> usize {
        let before = self.orders.len();
        self.orders.retain(|_, order| order.token.as_str() != token);
        let removed = before - self.orders.len();
        if removed > 0 {
            info!("Cleared {} tracked orders for token {}", removed, token);
        }
        removed
    }

    /// Clear all tracked orders
    pub fn clear(&mut self,
    ) {
        self.orders.clear();
    }

    /// Get order count
    pub fn count(&self) -> usize {
        self.orders.len()
    }
}

impl Default for OrderTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Order fill status
#[derive(Debug, Clone, PartialEq)]
pub enum FillStatus {
    Filled,
    Partial(f64), // filled size
    Cancelled,
    Failed,
    Pending,
}

/// Wait for order fill
/// Matches Python: _wait_for_fill()
pub async fn wait_for_fill<F, Fut>(
    order_id: &str,
    max_wait_secs: u64,
    mut check_fn: F,
) -> f64
where
    F: FnMut(&str) -> Fut,
    Fut: std::future::Future<Output = Option<FillStatus>>,
{
    for i in 0..max_wait_secs {
        sleep(Duration::from_secs(1)).await;

        match check_fn(order_id).await {
            Some(FillStatus::Filled) => {
                info!("✅ Order {} filled", order_id);
                return 1.0; // Assume full fill for now
            }
            Some(FillStatus::Partial(size)) => {
                info!("📊 Order {} partially filled: {}", order_id, size);
                return size;
            }
            Some(FillStatus::Cancelled) => {
                warn!("🗑️ Order {} cancelled", order_id);
                return 0.0;
            }
            Some(FillStatus::Failed) => {
                warn!("❌ Order {} failed", order_id);
                return 0.0;
            }
            Some(FillStatus::Pending) | None => {
                debug!("⏳ Order {} still pending... ({}/{})", order_id, i + 1, max_wait_secs);
            }
        }
    }

    warn!("⏰ Order {} wait timeout", order_id);
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_order() {
        let mut tracker = OrderTracker::new();

        tracker.track_order(
            "token_1".to_string(),
            "order_123".to_string(),
            "BUY".to_string(),
            0.5,
            10.0,
        );

        assert_eq!(tracker.count(), 1);

        let order = tracker.get_order("token_1").unwrap();
        assert_eq!(order.order_id, "order_123");
        assert_eq!(order.side, "BUY");
    }

    #[test]
    fn test_find_old_orders() {
        let mut tracker = OrderTracker::new();

        // Add an order
        tracker.track_order(
            "token_1".to_string(),
            "order_123".to_string(),
            "BUY".to_string(),
            0.5,
            10.0,
        );

        // Immediately check - should not be old
        let old = tracker.find_old_orders(0);
        assert!(old.is_empty());

        // Note: Can't easily test time-based filtering in unit tests
    }

    #[tokio::test]
    async fn test_wait_for_fill_success() {
        // Simplified test - just verify the function exists and works
        let check_fn = |_order_id: &str| async move {
            Some(FillStatus::Filled)
        };

        let filled = wait_for_fill("order_123", 10, check_fn).await;
        assert_eq!(filled, 1.0);
    }

    #[tokio::test]
    async fn test_wait_for_fill_cancelled() {
        let check_fn = |_order_id: &str| async move { Some(FillStatus::Cancelled) };

        let filled = wait_for_fill("order_123", 10, check_fn).await;
        assert_eq!(filled, 0.0);
    }
}
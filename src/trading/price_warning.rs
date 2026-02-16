//! Price warning cooldown mechanism
//! Matches Python: _should_log_price_warning(), _last_price_warnings

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::warn;

/// Price warning tracker with cooldown
pub struct PriceWarningTracker {
    /// Last warning time for each price key
    last_warnings: HashMap<String, Instant>,
    /// Cooldown duration in seconds
    cooldown_secs: u64,
}

impl PriceWarningTracker {
    /// Create new tracker with specified cooldown
    pub fn new(cooldown_secs: u64) -> Self {
        Self {
            last_warnings: HashMap::new(),
            cooldown_secs,
        }
    }
    
    /// Check if should log price warning
    /// Matches Python: _should_log_price_warning()
    pub fn should_warn(
        &mut self,
        price: f64,
        side: &str,
    ) -> bool {
        let key = format!("{}_{:.2}", side, price);
        let now = Instant::now();
        
        match self.last_warnings.get(&key) {
            Some(last_time) => {
                let elapsed = now.duration_since(*last_time).as_secs();
                if elapsed >= self.cooldown_secs {
                    // Update last warning time
                    self.last_warnings.insert(key, now);
                    true
                } else {
                    false
                }
            }
            None => {
                // First warning for this price
                self.last_warnings.insert(key, now);
                true
            }
        }
    }
    
    /// Log price warning with cooldown
    pub fn log_price_warning(
        &mut self,
        price: f64,
        side: &str,
        safe_low: f64,
        safe_high: f64,
        context: &str,
    ) {
        if self.should_warn(price, side) {
            if price < safe_low {
                warn!(
                    "⚠️ {} Price {:.4} below safe range [{:.2}, {:.2}]",
                    context, price, safe_low, safe_high
                );
            } else if price > safe_high {
                warn!(
                    "⚠️ {} Price {:.4} above safe range [{:.2}, {:.2}]",
                    context, price, safe_low, safe_high
                );
            }
        }
    }
    
    /// Clear all warning history
    pub fn clear(&mut self,
    ) {
        self.last_warnings.clear();
    }
    
    /// Get number of tracked warnings
    pub fn count(&self) -> usize {
        self.last_warnings.len()
    }
    
    /// Remove expired entries
    pub fn cleanup(&mut self,
    ) {
        let now = Instant::now();
        let cooldown = Duration::from_secs(self.cooldown_secs);
        
        self.last_warnings.retain(|_, last_time| {
            now.duration_since(*last_time) < cooldown * 2
        });
    }
}

impl Default for PriceWarningTracker {
    fn default() -> Self {
        Self::new(60) // Default 60 second cooldown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_should_warn_first_time() {
        let mut tracker = PriceWarningTracker::new(60);
        assert!(tracker.should_warn(0.05, "below"));
    }
    
    #[test]
    fn test_should_warn_cooldown() {
        let mut tracker = PriceWarningTracker::new(60);
        
        // First warning should pass
        assert!(tracker.should_warn(0.05, "below"));
        
        // Immediate second warning should be blocked
        assert!(!tracker.should_warn(0.05, "below"));
    }
    
    #[test]
    fn test_different_prices() {
        let mut tracker = PriceWarningTracker::new(60);
        
        // Different prices should have separate cooldowns
        assert!(tracker.should_warn(0.05, "below"));
        assert!(tracker.should_warn(0.06, "below"));
    }
}
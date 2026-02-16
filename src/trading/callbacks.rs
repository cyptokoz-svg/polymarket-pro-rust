//! Callback system for order operations
//! Matches Python: set_order_callbacks()

/// Order information
#[derive(Debug, Clone)]
pub struct OrderInfo {
    pub order_id: String,
    pub token_id: String,
    pub side: String,
    pub price: f64,
    pub size: f64,
    pub status: String,
}

/// Callback manager for order operations
pub struct CallbackManager {
    has_create_order: bool,
    has_cancel_order: bool,
    has_get_orders: bool,
}

impl CallbackManager {
    /// Create new callback manager
    pub fn new() -> Self {
        Self {
            has_create_order: false,
            has_cancel_order: false,
            has_get_orders: false,
        }
    }
    
    /// Set order creation callback
    pub fn set_create_order_callback(
        &mut self,
    ) {
        tracing::info!("✅ Set create_order callback");
        self.has_create_order = true;
    }
    
    /// Set order cancellation callback
    pub fn set_cancel_order_callback(
        &mut self,
    ) {
        tracing::info!("✅ Set cancel_order callback");
        self.has_cancel_order = true;
    }
    
    /// Set get orders callback
    pub fn set_get_orders_callback(
        &mut self,
    ) {
        tracing::info!("✅ Set get_orders callback");
        self.has_get_orders = true;
    }
    
    /// Check if create order callback is set
    pub fn has_create_order(&self,
    ) -> bool {
        self.has_create_order
    }
    
    /// Check if cancel order callback is set
    pub fn has_cancel_order(&self,
    ) -> bool {
        self.has_cancel_order
    }
    
    /// Check if get orders callback is set
    pub fn has_get_orders(&self,
    ) -> bool {
        self.has_get_orders
    }
}

impl Default for CallbackManager {
    fn default() -> Self {
        Self::new()
    }
}
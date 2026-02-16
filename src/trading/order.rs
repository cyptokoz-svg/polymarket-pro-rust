//! CLOB Order signing and placement
//! Handles EIP-712 signing for Polymarket orders

use ethers::prelude::*;
use serde::{Deserialize, Serialize};

/// CLOB Order structure for signing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClobOrder {
    pub salt: u64,
    pub maker: Address,
    pub signer: Address,
    pub taker: Address,
    pub token_id: U256,
    pub maker_amount: U256,
    pub taker_amount: U256,
    pub expiration: u64,
    pub nonce: u64,
    pub fee_rate_bps: u32,
    pub side: OrderSide,
    pub signature_type: u8,
}

/// Order side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Order signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSignature {
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub v: u8,
}

/// Order signer
pub struct OrderSigner {
    chain_id: u64,
}

impl OrderSigner {
    /// Create new order signer
    pub fn new(chain_id: u64) -> Self {
        Self { chain_id }
    }
    
    /// Sign an order
    pub async fn sign_order(
        &self,
        order: &ClobOrder,
        wallet: &LocalWallet,
    ) -> Result<OrderSignature, Box<dyn std::error::Error>> {
        // Create EIP-712 domain
        let domain = eip712::EIP712Domain {
            name: "Polymarket CTF Exchange".to_string(),
            version: "1".to_string(),
            chain_id: self.chain_id,
            verifying_contract: Some("0x...".parse()?), // CTF Exchange contract
            salt: None,
        };
        
        // Hash the order data
        let order_hash = self.hash_order(order);
        
        // Sign the hash
        let signature = wallet.sign_hash(order_hash).await?;
        
        Ok(OrderSignature {
            r: signature.r.into(),
            s: signature.s.into(),
            v: signature.v as u8,
        })
    }
    
    /// Hash order data according to EIP-712
    fn hash_order(&self,
        _order: &ClobOrder,
    ) -> H256 {
        // TODO: Implement proper EIP-712 order hashing
        H256::zero()
    }
}

/// Order builder
pub struct OrderBuilder {
    maker: Address,
    chain_id: u64,
}

impl OrderBuilder {
    /// Create new order builder
    pub fn new(maker: Address, chain_id: u64) -> Self {
        Self { maker, chain_id }
    }
    
    /// Build a buy order with cryptographically secure salt
    pub fn build_buy_order(
        &self,
        token_id: U256,
        maker_amount: U256,  // USDC amount
        taker_amount: U256,  // Token amount
        expiration: u64,
        nonce: u64,
    ) -> ClobOrder {
        // Use cryptographically secure random number generator
        let mut salt_bytes = [0u8; 8];
        rand::rngs::OsRng.fill_bytes(&mut salt_bytes);
        let salt = u64::from_le_bytes(salt_bytes);
        
        ClobOrder {
            salt,
            maker: self.maker,
            signer: self.maker,
            taker: Address::zero(),
            token_id,
            maker_amount,
            taker_amount,
            expiration,
            nonce,
            fee_rate_bps: 0,
            side: OrderSide::Buy,
            signature_type: 0,
        }
    }
    
    /// Build a sell order with cryptographically secure salt
    pub fn build_sell_order(
        &self,
        token_id: U256,
        maker_amount: U256,  // Token amount
        taker_amount: U256,  // USDC amount
        expiration: u64,
        nonce: u64,
    ) -> ClobOrder {
        // Use cryptographically secure random number generator
        let mut salt_bytes = [0u8; 8];
        rand::rngs::OsRng.fill_bytes(&mut salt_bytes);
        let salt = u64::from_le_bytes(salt_bytes);
        
        ClobOrder {
            salt,
            maker: self.maker,
            signer: self.maker,
            taker: Address::zero(),
            token_id,
            maker_amount,
            taker_amount,
            expiration,
            nonce,
            fee_rate_bps: 0,
            side: OrderSide::Sell,
            signature_type: 0,
        }
    }
}

/// Convert price and size to maker/taker amounts
pub fn calculate_order_amounts(
    side: OrderSide,
    price: f64,
    size: f64,
    decimals: u8,
) -> (U256, U256) {
    let scale = U256::from(10).pow(U256::from(decimals));
    
    // Helper function to convert f64 to U256 without precision loss
    fn f64_to_u256(value: f64) -> U256 {
        if value.is_nan() || value.is_infinite() || value < 0.0 {
            return U256::ZERO;
        }
        // Use string conversion to avoid truncation
        let value_str = format!("{:.0}", value);
        U256::from_str_radix(&value_str, 10).unwrap_or(U256::ZERO)
    }
    
    match side {
        OrderSide::Buy => {
            // Buy: maker_amount = USDC to spend, taker_amount = tokens to receive
            let usdc_amount = f64_to_u256(price * size * 10f64.powi(decimals as i32));
            let token_amount = f64_to_u256(size * 10f64.powi(decimals as i32));
            (usdc_amount * scale, token_amount * scale)
        }
        OrderSide::Sell => {
            // Sell: maker_amount = tokens to sell, taker_amount = USDC to receive
            let token_amount = f64_to_u256(size * 10f64.powi(decimals as i32));
            let usdc_amount = f64_to_u256(price * size * 10f64.powi(decimals as i32));
            (token_amount * scale, usdc_amount * scale)
        }
    }
}
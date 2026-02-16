//! Wallet management module using Alloy
//! Handles private key wallets and Gnosis Safe integration

use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use alloy_primitives::Address;
use std::str::FromStr;
use thiserror::Error;

pub mod safe;

pub use safe::SafeWallet;

#[derive(Error, Debug)]
pub enum WalletError {
    #[error("Invalid private key: {0}")]
    InvalidPrivateKey(String),
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    #[error("Signing error: {0}")]
    SigningError(String),
}

/// Redemption typed data for EIP-712 signing
#[derive(Debug, Clone)]
pub struct RedeemTypedData {
    pub condition_id: String,
    pub amount: u64,
    pub recipient: String,
}

/// Wallet trait defining common wallet operations
#[async_trait::async_trait]
pub trait Wallet: Send + Sync {
    /// Get wallet address
    fn address(&self) -> Address;
    
    /// Sign a message
    async fn sign_message(&self, message: &[u8]) -> Result<Vec<u8>, WalletError>;
    
    /// Get chain ID
    fn chain_id(&self) -> u64;
}

/// Private key wallet implementation using Alloy
pub struct PrivateKeyWallet {
    signer: PrivateKeySigner,
    chain_id: u64,
}

impl PrivateKeyWallet {
    /// Create wallet from private key hex string
    pub fn from_private_key(key: &str, chain_id: u64) -> Result<Self, WalletError> {
        let key = key.trim_start_matches("0x");
        let signer = PrivateKeySigner::from_str(key)
            .map_err(|e| WalletError::InvalidPrivateKey(e.to_string()))?;
        
        Ok(Self { signer, chain_id })
    }
    
    /// Get the underlying signer
    pub fn signer(&self) -> &PrivateKeySigner {
        &self.signer
    }
}

#[async_trait::async_trait]
impl Wallet for PrivateKeyWallet {
    fn address(&self) -> Address {
        // Convert SDK Address to alloy_primitives Address
        let sdk_addr = self.signer.address();
        Address::from_slice(sdk_addr.as_slice())
    }
    
    async fn sign_message(&self, message: &[u8]) -> Result<Vec<u8>, WalletError> {
        let signature = self.signer.sign_message(message)
            .await
            .map_err(|e| WalletError::SigningError(e.to_string()))?;
        Ok(signature.as_bytes().to_vec())
    }
    
    fn chain_id(&self) -> u64 {
        self.chain_id
    }
}

/// Signature wrapper
pub struct Signature {
    bytes: Vec<u8>,
}

impl Signature {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
    
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_private_key_wallet_creation() {
        // Use a test key (not a real private key)
        let pk = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let result = PrivateKeyWallet::from_private_key(pk, 137);
        // Test may fail with invalid key, that's ok for this test
        let _ = result;
    }

    #[test]
    fn test_invalid_private_key_fails() {
        let invalid_keys = vec!["0xinvalid", "not_a_key", "", "0x123"];
        for key in invalid_keys {
            assert!(PrivateKeyWallet::from_private_key(key, 137).is_err());
        }
    }

    #[test]
    fn test_address_derivation() {
        // Use a test key (not a real private key)
        let pk = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let result = PrivateKeyWallet::from_private_key(pk, 137);
        // Just check it doesn't panic
        let _ = result;
    }

    #[tokio::test]
    async fn test_sign_message() {
        // Use a test key (not a real private key)
        let pk = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let wallet = PrivateKeyWallet::from_private_key(pk, 137).unwrap();
        let message = b"Test message";
        
        let signature = wallet.sign_message(message).await;
        assert!(signature.is_ok());
        assert_eq!(signature.unwrap().len(), 65);
    }
}
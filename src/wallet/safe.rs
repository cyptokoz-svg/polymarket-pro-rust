//! Gnosis Safe wallet integration
//! Supports gasless transactions via Builder Relayer

use alloy_primitives::Address;
use crate::wallet::WalletError;

/// Gnosis Safe wallet
pub struct SafeWallet {
    address: Address,
    owner: Address,
    nonce: u64,
}

impl SafeWallet {
    /// Create new Safe wallet
    pub fn new(address: &str, owner: &str) -> Result<Self, WalletError> {
        let address = address.parse::<Address>()
            .map_err(|e| WalletError::InvalidAddress(e.to_string()))?;
        let owner = owner.parse::<Address>()
            .map_err(|e| WalletError::InvalidAddress(e.to_string()))?;
        
        Ok(Self {
            address,
            owner,
            nonce: 0,
        })
    }
    
    /// Get Safe address
    pub fn address(&self) -> Address {
        self.address
    }
    
    /// Get owner address
    pub fn owner(&self) -> Address {
        self.owner
    }
    
    /// Check if owner is valid
    pub fn is_owner_valid(&self) -> bool {
        self.owner != Address::ZERO
    }
    
    /// Get current nonce
    pub fn nonce(&self) -> u64 {
        self.nonce
    }
    
    /// Increment nonce after transaction
    pub fn increment_nonce(&mut self) {
        self.nonce += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_wallet_creation() {
        let safe_addr = "0x45dCeb24119296fB57D06d83c1759cC191c3c96E";
        let owner = "0xB18Ec66081b444037F7C1B5ffEE228693B854E7A";
        
        let safe = SafeWallet::new(safe_addr, owner);
        assert!(safe.is_ok());
        
        let safe = safe.unwrap();
        assert!(safe.is_owner_valid());
        assert_eq!(safe.nonce(), 0);
    }

    #[test]
    fn test_invalid_address_fails() {
        let result = SafeWallet::new("invalid", "0xB18Ec66081b444037F7C1B5ffEE228693B854E7A");
        assert!(result.is_err());
    }

    #[test]
    fn test_nonce_increment() {
        let safe_addr = "0x45dCeb24119296fB57D06d83c1759cC191c3c96E";
        let owner = "0xB18Ec66081b444037F7C1B5ffEE228693B854E7A";
        let mut safe = SafeWallet::new(safe_addr, owner).unwrap();
        
        assert_eq!(safe.nonce(), 0);
        safe.increment_nonce();
        assert_eq!(safe.nonce(), 1);
        safe.increment_nonce();
        assert_eq!(safe.nonce(), 2);
    }
}
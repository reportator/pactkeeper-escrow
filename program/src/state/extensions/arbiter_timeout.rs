use alloc::vec::Vec;
use pinocchio::{
    error::ProgramError,
    sysvars::{clock::Clock, Sysvar},
};

use crate::{assert_no_padding, require_len, traits::ExtensionData};

/// PactKeeper: safety valve for the arbiter (oracle).
///
/// When the arbiter has not acted on a deposit for `timeout` seconds after it was made,
/// the depositor may withdraw it (full refund) without the arbiter's signature.
/// Protects the client's funds if the oracle is gone; the arbiter can still Settle until then
/// (and afterwards, until the depositor withdraws).
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct ArbiterTimeoutData {
    pub timeout: u64,
}

assert_no_padding!(ArbiterTimeoutData, 8);

impl ArbiterTimeoutData {
    pub const LEN: usize = 8;

    pub fn new(timeout: u64) -> Self {
        Self { timeout }
    }

    /// Whether `now` is at or past `deposited_at + timeout`.
    pub fn is_expired_at(&self, deposited_at: i64, now: i64) -> Result<bool, ProgramError> {
        let timeout = i64::try_from(self.timeout).map_err(|_| ProgramError::ArithmeticOverflow)?;
        let expires_at = deposited_at.checked_add(timeout).ok_or(ProgramError::ArithmeticOverflow)?;
        Ok(now >= expires_at)
    }

    /// Whether the arbiter timeout for a deposit made at `deposited_at` has passed (reads the Clock sysvar).
    pub fn is_expired(&self, deposited_at: i64) -> Result<bool, ProgramError> {
        self.is_expired_at(deposited_at, Clock::get()?.unix_timestamp)
    }
}

impl ExtensionData for ArbiterTimeoutData {
    fn to_bytes(&self) -> Vec<u8> {
        self.timeout.to_le_bytes().to_vec()
    }

    fn from_bytes(data: &[u8]) -> Result<Self, ProgramError> {
        require_len!(data, Self::LEN);

        Ok(Self { timeout: u64::from_le_bytes(data[0..8].try_into().unwrap()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arbiter_timeout_roundtrip() {
        let data = ArbiterTimeoutData::new(30 * 24 * 3600);
        assert_eq!(ArbiterTimeoutData::from_bytes(&data.to_bytes()).unwrap(), data);
    }

    #[test]
    fn test_arbiter_timeout_expiry_boundary() {
        let data = ArbiterTimeoutData::new(100);
        assert!(!data.is_expired_at(1_000, 1_099).unwrap());
        assert!(data.is_expired_at(1_000, 1_100).unwrap());
        assert!(data.is_expired_at(1_000, 5_000).unwrap());
    }

    #[test]
    fn test_arbiter_timeout_overflow() {
        let data = ArbiterTimeoutData::new(u64::MAX);
        assert!(data.is_expired_at(0, 0).is_err());
        let data = ArbiterTimeoutData::new(i64::MAX as u64);
        assert!(data.is_expired_at(1, 1).is_err());
    }

    #[test]
    fn test_arbiter_timeout_from_bytes_wrong_len() {
        assert!(ArbiterTimeoutData::from_bytes(&[1, 2, 3]).is_err());
    }
}

use alloc::vec::Vec;
use pinocchio::{account::AccountView, error::ProgramError, ProgramResult};

use crate::{assert_no_padding, errors::EscrowProgramError, require_len, traits::ExtensionData};

/// Beneficiary extension data (stored in TLV format)
///
/// PactKeeper: the counterparty (contractor) who receives the agreed share of a deposit
/// when the arbiter settles it. Fixed by the admin before the escrow is made immutable,
/// so the arbiter can only decide *how much* goes to the beneficiary, never *to whom*.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct BeneficiaryData {
    pub beneficiary: pinocchio::Address,
}

assert_no_padding!(BeneficiaryData, 32);

impl BeneficiaryData {
    pub const LEN: usize = 32;

    pub fn new(beneficiary: pinocchio::Address) -> Self {
        Self { beneficiary }
    }

    /// The beneficiary account passed to Settle must be the stored one.
    pub fn validate(&self, beneficiary: &AccountView) -> ProgramResult {
        if beneficiary.address() != &self.beneficiary {
            return Err(EscrowProgramError::InvalidBeneficiary.into());
        }
        Ok(())
    }
}

impl ExtensionData for BeneficiaryData {
    fn to_bytes(&self) -> Vec<u8> {
        self.beneficiary.as_array().to_vec()
    }

    fn from_bytes(data: &[u8]) -> Result<Self, ProgramError> {
        require_len!(data, Self::LEN);

        Ok(Self { beneficiary: pinocchio::Address::new_from_array(data[0..32].try_into().unwrap()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio::Address;

    #[test]
    fn test_beneficiary_data_roundtrip() {
        let data = BeneficiaryData::new(Address::new_from_array([7u8; 32]));
        let parsed = BeneficiaryData::from_bytes(&data.to_bytes()).unwrap();
        assert_eq!(parsed, data);
    }

    #[test]
    fn test_beneficiary_data_wrong_len() {
        assert!(BeneficiaryData::from_bytes(&[1u8; 31]).is_err());
    }
}

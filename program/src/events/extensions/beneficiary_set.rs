use alloc::vec::Vec;
use codama::CodamaType;
use pinocchio::Address;

use crate::traits::{EventDiscriminator, EventDiscriminators, EventSerialize};

#[derive(CodamaType)]
pub struct BeneficiarySetEvent {
    pub escrow: Address,
    pub beneficiary: Address,
}

impl EventDiscriminator for BeneficiarySetEvent {
    const DISCRIMINATOR: u8 = EventDiscriminators::BeneficiarySet as u8;
}

impl EventSerialize for BeneficiarySetEvent {
    #[inline(always)]
    fn to_bytes_inner(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(Self::DATA_LEN);
        data.extend_from_slice(self.escrow.as_ref());
        data.extend_from_slice(self.beneficiary.as_ref());
        data
    }
}

impl BeneficiarySetEvent {
    pub const DATA_LEN: usize = 32 + 32; // escrow + beneficiary

    #[inline(always)]
    pub fn new(escrow: Address, beneficiary: Address) -> Self {
        Self { escrow, beneficiary }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EVENT_IX_TAG_LE;
    use crate::traits::EVENT_DISCRIMINATOR_LEN;

    #[test]
    fn test_beneficiary_set_event_to_bytes() {
        let event = BeneficiarySetEvent::new(Address::new_from_array([1u8; 32]), Address::new_from_array([2u8; 32]));
        let bytes = event.to_bytes();
        assert_eq!(bytes.len(), EVENT_DISCRIMINATOR_LEN + BeneficiarySetEvent::DATA_LEN);
        assert_eq!(&bytes[..8], EVENT_IX_TAG_LE);
        assert_eq!(bytes[8], EventDiscriminators::BeneficiarySet as u8);
    }
}

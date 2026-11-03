use alloc::vec::Vec;
use codama::CodamaType;
use pinocchio::Address;

use crate::traits::{EventDiscriminator, EventDiscriminators, EventSerialize};

#[derive(CodamaType)]
pub struct ArbiterTimeoutSetEvent {
    pub escrow: Address,
    pub timeout: u64,
}

impl EventDiscriminator for ArbiterTimeoutSetEvent {
    const DISCRIMINATOR: u8 = EventDiscriminators::ArbiterTimeoutSet as u8;
}

impl EventSerialize for ArbiterTimeoutSetEvent {
    #[inline(always)]
    fn to_bytes_inner(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(Self::DATA_LEN);
        data.extend_from_slice(self.escrow.as_ref());
        data.extend_from_slice(&self.timeout.to_le_bytes());
        data
    }
}

impl ArbiterTimeoutSetEvent {
    pub const DATA_LEN: usize = 32 + 8; // escrow + timeout

    #[inline(always)]
    pub fn new(escrow: Address, timeout: u64) -> Self {
        Self { escrow, timeout }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EVENT_IX_TAG_LE;
    use crate::traits::EVENT_DISCRIMINATOR_LEN;

    #[test]
    fn test_arbiter_timeout_set_event_to_bytes() {
        let escrow = Address::new_from_array([7u8; 32]);
        let bytes = ArbiterTimeoutSetEvent::new(escrow, 86_400).to_bytes();
        assert_eq!(bytes.len(), EVENT_DISCRIMINATOR_LEN + ArbiterTimeoutSetEvent::DATA_LEN);
        assert_eq!(&bytes[..8], EVENT_IX_TAG_LE);
        assert_eq!(bytes[8], EventDiscriminators::ArbiterTimeoutSet as u8);
        assert_eq!(&bytes[9..41], escrow.as_ref());
        assert_eq!(&bytes[41..49], &86_400u64.to_le_bytes());
    }
}

use alloc::vec::Vec;
use codama::CodamaType;
use pinocchio::Address;

use crate::traits::{EventDiscriminator, EventDiscriminators, EventSerialize};

/// PactKeeper: a deposit split by the arbiter between the beneficiary and the depositor.
/// `receipt_seed` is the SHA-256 of the agreed contract text.
#[derive(CodamaType)]
pub struct SettleEvent {
    pub escrow: Address,
    pub depositor: Address,
    pub beneficiary: Address,
    pub mint: Address,
    pub receipt_seed: Address,
    pub amount: u64,
    pub payout_bps: u16,
    pub payout_amount: u64,
}

impl EventDiscriminator for SettleEvent {
    const DISCRIMINATOR: u8 = EventDiscriminators::Settle as u8;
}

impl EventSerialize for SettleEvent {
    #[inline(always)]
    fn to_bytes_inner(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(Self::DATA_LEN);
        data.extend_from_slice(self.escrow.as_ref());
        data.extend_from_slice(self.depositor.as_ref());
        data.extend_from_slice(self.beneficiary.as_ref());
        data.extend_from_slice(self.mint.as_ref());
        data.extend_from_slice(self.receipt_seed.as_ref());
        data.extend_from_slice(&self.amount.to_le_bytes());
        data.extend_from_slice(&self.payout_bps.to_le_bytes());
        data.extend_from_slice(&self.payout_amount.to_le_bytes());
        data
    }
}

impl SettleEvent {
    pub const DATA_LEN: usize = 32 * 5 + 8 + 2 + 8;

    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        escrow: Address,
        depositor: Address,
        beneficiary: Address,
        mint: Address,
        receipt_seed: Address,
        amount: u64,
        payout_bps: u16,
        payout_amount: u64,
    ) -> Self {
        Self { escrow, depositor, beneficiary, mint, receipt_seed, amount, payout_bps, payout_amount }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EVENT_IX_TAG_LE;
    use crate::traits::EVENT_DISCRIMINATOR_LEN;

    #[test]
    fn test_settle_event_to_bytes() {
        let a = |b: u8| Address::new_from_array([b; 32]);
        let event = SettleEvent::new(a(1), a(2), a(3), a(4), a(5), 1000, 7500, 750);
        let bytes = event.to_bytes();
        assert_eq!(bytes.len(), EVENT_DISCRIMINATOR_LEN + SettleEvent::DATA_LEN);
        assert_eq!(&bytes[..8], EVENT_IX_TAG_LE);
        assert_eq!(bytes[8], EventDiscriminators::Settle as u8);
        let inner = event.to_bytes_inner();
        assert_eq!(&inner[160..168], &1000u64.to_le_bytes());
        assert_eq!(&inner[168..170], &7500u16.to_le_bytes());
        assert_eq!(&inner[170..178], &750u64.to_le_bytes());
    }
}
